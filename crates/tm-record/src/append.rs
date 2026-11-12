//! The main-chain append (item 39; 11.21 Batch write; rule 4). The
//! `&OwnerCap` argument arrives with `tm-owner-cap` at P3 (item 43); until
//! then the entry point takes the held [`Lock`] and a framed [`Batch`].

use std::fs::OpenOptions;
use std::io::Write as _;

use serde_json::Value;
use tm_formats::envelope::{self, Actor, Source};
use tm_formats::events::KnownEvent;
use tm_formats::id::EventId;

use crate::head::Head;
use crate::lock::Lock;
use crate::segment::{self, BatchHeader, payload_hash, plain_path};
use crate::{Code, Error, Record, Result, fsync_dir};

/// A framed batch: validated lines that link to one another, with their
/// header computed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Batch {
    lines: Vec<String>,
    ids: Vec<EventId>,
    first_seq: u64,
    first_prev: EventId,
    number: u64,
    epoch: u64,
    payload: Vec<u8>,
}

impl Batch {
    /// Frame lines the Core built with [`envelope::write_line`]. Every line
    /// must be a main-chain envelope; together they must carry one `batch`
    /// and one `epoch`, consecutive `seq`, and `prev` links within the batch.
    ///
    /// # Errors
    /// `E1002` for a line that does not read; `E2102` for lines that do
    /// not link.
    pub fn new(lines: Vec<String>) -> Result<Batch> {
        if lines.is_empty() {
            return Err(Error::code(Code::E2102, "a batch holds at least one event"));
        }
        let mut ids = Vec::with_capacity(lines.len());
        let mut first_seq = 0;
        let mut first_prev: Option<EventId> = None;
        let mut number = 0;
        let mut epoch = 0;
        let mut payload = Vec::new();
        for (i, text) in lines.iter().enumerate() {
            if text.contains('\n') {
                return Err(Error::code(
                    Code::E2102,
                    format!("line {i} contains a newline"),
                ));
            }
            let line = envelope::read_line(text)?;
            let int = |k: &str| line.envelope[k].as_u64();
            let chain = line.envelope["chain"].as_str().unwrap_or_default();
            let (Some(seq), Some(b), Some(ep)) = (int("seq"), int("batch"), int("epoch")) else {
                return Err(Error::code(
                    Code::E2102,
                    format!("line {i} is not a main-chain envelope"),
                ));
            };
            if chain != "main" {
                return Err(Error::code(
                    Code::E2102,
                    format!("line {i} is on chain {chain}, not main"),
                ));
            }
            let prev = line.envelope["prev"]
                .as_str()
                .and_then(EventId::parse)
                .ok_or_else(|| Error::code(Code::E2102, format!("line {i} has no prev")))?;
            if i == 0 {
                first_seq = seq;
                first_prev = Some(prev);
                number = b;
                epoch = ep;
            } else {
                if b != number || ep != epoch {
                    return Err(Error::code(
                        Code::E2102,
                        format!("line {i} carries another batch or epoch"),
                    ));
                }
                if seq != first_seq + i as u64 {
                    return Err(Error::code(
                        Code::E2102,
                        format!("line {i} has seq {seq}, expected {}", first_seq + i as u64),
                    ));
                }
                if Some(&prev) != ids.last() {
                    return Err(Error::code(
                        Code::E2102,
                        format!("line {i} does not link to line {}", i - 1),
                    ));
                }
            }
            payload.extend_from_slice(text.as_bytes());
            payload.push(b'\n');
            ids.push(line.id);
        }
        Ok(Batch {
            lines,
            ids,
            first_seq,
            first_prev: first_prev.unwrap_or_else(|| unreachable!("set on the first line")),
            number,
            epoch,
            payload,
        })
    }

    /// Build a batch of known events on top of `head` (what the Core does
    /// before calling [`Record::append`]): `seq`, `prev`, `batch`, and
    /// `epoch` follow from `head` and `epoch`.
    ///
    /// # Errors
    /// `E1002` from the envelope writer.
    pub fn build(
        head: &Head,
        epoch: u64,
        at: &str,
        actor: &Actor,
        src: Option<&Source>,
        events: &[KnownEvent],
    ) -> Result<Batch> {
        let mut lines = Vec::with_capacity(events.len());
        let mut prev = head.last_id.clone();
        for (i, ev) in events.iter().enumerate() {
            let h = envelope::Head {
                chain: "main".into(),
                seq: head.seq + 1 + i as u64,
                prev: prev.clone(),
                at: at.to_owned(),
                actor: actor.clone(),
                src: src.cloned(),
                batch: Some(head.batch + 1),
                epoch: Some(epoch),
            };
            let (line, id) = envelope::write_line(&h, ev)?;
            lines.push(line);
            prev = id;
        }
        Batch::new(lines)
    }

    /// The lines, in order.
    #[must_use]
    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    /// The event IDs, in order.
    #[must_use]
    pub fn ids(&self) -> &[EventId] {
        &self.ids
    }

    /// The batch number the lines carry.
    #[must_use]
    pub fn number(&self) -> u64 {
        self.number
    }

    /// The epoch the lines carry.
    #[must_use]
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// The header for this batch (11.3).
    #[must_use]
    pub fn header(&self) -> BatchHeader {
        BatchHeader {
            b: self.number,
            n: self.lines.len() as u64,
            len: self.payload.len() as u64,
            h: payload_hash(&self.payload),
            epoch: self.epoch,
        }
    }

    /// The payload bytes.
    #[must_use]
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    /// Bytes this batch adds to a segment: header line plus payload.
    #[must_use]
    pub fn framed_len(&self) -> u64 {
        self.header().line().len() as u64 + self.payload.len() as u64
    }

    /// The body of event `i`, for callers that queued events and want them
    /// back.
    #[must_use]
    pub fn body(&self, i: usize) -> Option<Value> {
        let text = self.lines.get(i)?;
        envelope::read_line(text)
            .ok()
            .map(|l| l.envelope["body"].clone())
    }
}

/// The batch carries the lock's epoch and continues `HEAD`.
fn check_continues(head: &Head, lock: &Lock, batch: &Batch) -> Result<()> {
    if batch.epoch != lock.epoch() {
        return Err(Error::code(
            Code::E2102,
            format!(
                "batch carries epoch {}, the lock's is {}",
                batch.epoch,
                lock.epoch()
            ),
        ));
    }
    if batch.number != head.batch + 1
        || batch.first_seq != head.seq + 1
        || batch.first_prev != head.last_id
    {
        return Err(Error::code(
            Code::E2102,
            format!(
                "batch {} (first seq {}, prev {}) does not continue HEAD (batch {}, seq {}, last_id {})",
                batch.number, batch.first_seq, batch.first_prev, head.batch, head.seq, head.last_id
            ),
        ));
    }
    Ok(())
}

impl Record {
    /// Append one batch under the lock (11.21 Batch write), with the
    /// pre-append fencing re-read of 3.1. Returns the new `HEAD`.
    ///
    /// # Errors
    /// `E2102` when `HEAD` has another epoch, the open segment's length (in
    /// whichever form exists) is not `HEAD.offset`, or the batch does not
    /// continue the chain; `E2103` when the segment the writer would roll to
    /// already exists; `E2008` on a read-only record; `Error::Io`.
    pub fn append(&self, lock: &Lock, batch: &Batch) -> Result<Head> {
        self.writable()?;
        let main = self.main_dir();
        // Fencing: a fresh HEAD read, the epoch, the open segment's length.
        let head = Head::read(&main)?;
        if head.epoch != lock.epoch() {
            return Err(Error::code(
                Code::E2102,
                format!(
                    "HEAD.epoch is {}, this writer's is {}: another writer took the lock",
                    head.epoch,
                    lock.epoch()
                ),
            ));
        }
        let (plain, packed) = segment::forms(&main, head.segment);
        let len = segment::open_len(&main, head.segment)?;
        if len != head.offset {
            return Err(Error::code(
                Code::E2102,
                format!(
                    "segment {} is {len} bytes but HEAD.offset is {}: refusing to write over a fork",
                    head.segment, head.offset
                ),
            ));
        }
        check_continues(&head, lock, batch)?;
        // Step 0: finish a sealing that stopped early, then seal the open
        // segment if this batch would carry it past the limit. A HEAD.segment
        // that is already packed-only was sealed by a writer that died before
        // its HEAD rename (3.1): roll without sealing again.
        for n in 1..head.segment {
            segment::resume_seal(&main, n)?;
        }
        let mut segment_no = head.segment;
        let mut offset = head.offset;
        let sealed_already = packed && !plain;
        if sealed_already || (offset > 0 && offset + batch.framed_len() > self.segment_limit) {
            if !sealed_already {
                segment::seal(&main, segment_no)?;
            }
            segment_no += 1;
            offset = 0;
            if segment::forms(&main, segment_no) != (false, false) {
                return Err(Error::code(
                    Code::E2103,
                    format!("segment {segment_no} exists before the chain reached it"),
                ));
            }
        }
        // Steps 1 and 2: header line and event lines, one write each, fsync.
        let header = batch.header();
        let path = plain_path(&main, segment_no);
        let mut file = OpenOptions::new()
            .append(true)
            .create(true)
            .open(&path)
            .map_err(|e| Error::io(&path, e))?;
        file.write_all(header.line().as_bytes())
            .map_err(|e| Error::io(&path, e))?;
        for line in &batch.lines {
            file.write_all(line.as_bytes())
                .map_err(|e| Error::io(&path, e))?;
            file.write_all(b"\n").map_err(|e| Error::io(&path, e))?;
        }
        file.sync_all().map_err(|e| Error::io(&path, e))?;
        if offset == 0 {
            fsync_dir(&main)?;
        }
        // Step 3: HEAD, with the lock re-read before the rename.
        let new_head = Head {
            seq: batch.first_seq + batch.lines.len() as u64 - 1,
            segment: segment_no,
            offset: offset + batch.framed_len(),
            last_id: batch
                .ids
                .last()
                .cloned()
                .unwrap_or_else(|| head.last_id.clone()),
            batch: batch.number,
            epoch: lock.epoch(),
            format: 1,
        };
        new_head.write_checked(&main, Some(lock.epoch()))?;
        Ok(new_head)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lock::LockOptions;
    use serde_json::json;
    use std::time::Duration;
    use tm_formats::envelope::{ActorKind, SourceKind};
    use tm_formats::events::EventType;

    const AT: &str = "2026-11-11T12:00:00Z";

    fn actor() -> Actor {
        Actor {
            kind: ActorKind::Owner,
            id: "tim".into(),
        }
    }

    fn src() -> Source {
        Source {
            source: SourceKind::Cli,
            tty: true,
            harness_env: false,
            confirmed_by: None,
        }
    }

    fn event(i: u64) -> KnownEvent {
        KnownEvent::new(
            EventType::SettingsChanged,
            &json!({"key": "view.rows_per_page", "value": {"t": "int", "v": i.to_string()}, "previous": null}),
        )
        .unwrap()
    }

    fn opts() -> LockOptions {
        LockOptions {
            heartbeat: Duration::from_millis(200),
            takeover: Duration::from_secs(5),
            wait: Some(Duration::from_secs(5)),
            poll: Duration::from_millis(20),
            host: Some("test".into()),
        }
    }

    #[test]
    fn append_links_seals_and_fences() {
        let dir = tempfile::tempdir().unwrap();
        let r = Record::create(dir.path(), "p", "o", AT)
            .unwrap()
            .with_segment_limit(1200);
        assert_eq!(
            Record::create(dir.path(), "p", "o", AT)
                .unwrap_err()
                .catalog_code(),
            Some(Code::E2001)
        );
        let lock = r.lock(&opts(), |_, _| {}).unwrap();
        assert_eq!(lock.epoch(), 1);
        assert_eq!(
            Head::read(&r.main_dir()).unwrap().epoch,
            1,
            "epoch written to HEAD at lock time"
        );
        let b1 = Batch::build(
            lock.head_at_lock(),
            1,
            AT,
            &actor(),
            Some(&src()),
            &[event(1), event(2)],
        )
        .unwrap();
        assert_eq!(b1.number(), 1);
        assert_eq!(b1.header().n, 2);
        let h1 = r.append(&lock, &b1).unwrap();
        assert_eq!((h1.seq, h1.batch, h1.segment), (2, 1, 1));
        assert_eq!(h1.offset, b1.framed_len());
        // A batch that does not continue HEAD is refused.
        let stale = r.append(&lock, &b1).unwrap_err();
        assert_eq!(stale.catalog_code(), Some(Code::E2102));
        // The second batch would pass the limit: the first segment is sealed.
        let b2 = Batch::build(&h1, 1, AT, &actor(), Some(&src()), &[event(3), event(4)]).unwrap();
        let h2 = r.append(&lock, &b2).unwrap();
        assert_eq!((h2.seq, h2.batch, h2.segment), (4, 2, 2));
        assert_eq!(segment::forms(&r.main_dir(), 1), (false, true));
        // Batch::new validates links within a batch.
        let mut lines = b2.lines().to_vec();
        lines.swap(0, 1);
        assert_eq!(
            Batch::new(lines).unwrap_err().catalog_code(),
            Some(Code::E2102)
        );
        assert!(Batch::new(vec![]).is_err());
        assert_eq!(b2.body(0).unwrap()["key"], "view.rows_per_page");
        lock.release().unwrap();

        // Scans see four events across two segments; verify is clean.
        let events: Vec<_> = r.events().unwrap().map(|e| e.unwrap()).collect();
        assert_eq!(events.len(), 4);
        assert_eq!(events[2].position.segment, 2);
        assert_eq!(r.events_of_type("settings.changed").unwrap().count(), 4);
        assert_eq!(r.read_at(events[3].position).unwrap().id(), events[3].id());
        assert_eq!(r.find_by_id(events[0].id()).unwrap().unwrap().seq, 1);
        assert!(r.find_by_id(&r.genesis()).unwrap().is_none());
        assert_eq!(r.verify().unwrap(), vec![]);

        // Fencing: a writer whose epoch HEAD no longer carries is refused.
        let lock2 = r.lock(&opts(), |_, _| {}).unwrap();
        assert_eq!(lock2.epoch(), 2);
        let mut forged = Head::read(&r.main_dir()).unwrap();
        forged.epoch = 3;
        forged.write(&r.main_dir()).unwrap();
        let b3 = Batch::build(
            lock2.head_at_lock(),
            2,
            AT,
            &actor(),
            Some(&src()),
            &[event(5)],
        )
        .unwrap();
        let err = r.append(&lock2, &b3).unwrap_err();
        assert_eq!(err.catalog_code(), Some(Code::E2102), "{err}");
        forged.epoch = 2;
        forged.write(&r.main_dir()).unwrap();
        // Segment length not at HEAD.offset: also refused.
        let seg = plain_path(&r.main_dir(), 2);
        let mut f = OpenOptions::new().append(true).open(&seg).unwrap();
        f.write_all(b"junk\n").unwrap();
        drop(f);
        let err = r.append(&lock2, &b3).unwrap_err();
        assert_eq!(err.catalog_code(), Some(Code::E2102), "{err}");
        drop(lock2);
        // The next lock truncates it as a torn tail.
        let lock3 = r.lock(&opts(), |_, _| {}).unwrap();
        assert_eq!(lock3.torn_tail().map(|t| t.bytes), Some(5));
        assert_eq!(
            segment::plain_len(&r.main_dir(), 2).unwrap(),
            Head::read(&r.main_dir()).unwrap().offset
        );
        drop(lock3);
    }

    #[test]
    fn newer_format_is_read_only() {
        let dir = tempfile::tempdir().unwrap();
        let r = Record::create(dir.path(), "p", "o", AT).unwrap();
        let mut v = r.header().to_json();
        v["format"] = json!(99);
        crate::write_small_json(&dir.path().join("record.json"), &v).unwrap();
        let r = Record::open(dir.path()).unwrap();
        assert!(r.is_read_only());
        match r.lock(&opts(), |_, _| {}) {
            Err(e) => {
                assert_eq!(e.catalog_code(), Some(Code::E2008));
                assert!(e.to_string().contains("99"), "{e}");
            }
            Ok(_) => panic!("a newer-format record took the lock"),
        }
        assert_eq!(r.events().unwrap().count(), 0);
        assert_eq!(
            Record::open(&dir.path().join("nowhere"))
                .unwrap_err()
                .catalog_code(),
            Some(Code::E2007)
        );
    }
}
