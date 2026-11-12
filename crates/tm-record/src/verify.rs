//! Integrity checks (item 38; G13; 3.1): torn tail, fork, corruption, and
//! object verification, reported as a list of findings, one variant per
//! item 41 tampering case plus `head_field_mismatch`. Within a batch the
//! checks run line count, line IDs, payload hash, then links, and the first
//! failure is the finding.

use std::fmt;

use sha2::{Digest, Sha256};
use tm_formats::id::EventId;

use crate::head::Head;
use crate::scan::{Position, StoredEvent, parse_stored};
use crate::segment::{self, FrameError, LoadError, id_from_digest, payload_hash, split_lines};
use crate::{Record, Result};

/// What verification found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FindingKind {
    /// A kept object's bytes do not hash to its address.
    KeptByteChanged,
    /// A batch has fewer lines than its header's `n`, or is short of `len`.
    SegmentLineRemoved,
    /// A line's bytes do not match its `id`, or framing bytes are not a header.
    SegmentByteChanged,
    /// A batch's payload does not hash to its header's `h`.
    BatchHashMismatch,
    /// Bytes beyond `HEAD.offset` in the open segment, or a plain segment
    /// after a packed-only `HEAD.segment` (a sealing whose batch never
    /// reached its `HEAD` rename, 3.1).
    TornTailBeyondHead,
    /// A `prev`, `seq`, `batch`, or `epoch` that does not follow its predecessor.
    ForkPrevMismatch,
    /// `HEAD.offset` beyond the segment's end or inside a batch.
    HeadOffsetMismatch,
    /// A packed segment that does not decode.
    PackedSegmentCorrupt,
    /// `HEAD.seq`, `last_id`, `batch`, or `segment` disagrees with the events.
    HeadFieldMismatch,
}

impl FindingKind {
    /// Every kind, in item 41's order.
    pub const ALL: [FindingKind; 9] = [
        FindingKind::KeptByteChanged,
        FindingKind::SegmentLineRemoved,
        FindingKind::SegmentByteChanged,
        FindingKind::BatchHashMismatch,
        FindingKind::TornTailBeyondHead,
        FindingKind::ForkPrevMismatch,
        FindingKind::HeadOffsetMismatch,
        FindingKind::PackedSegmentCorrupt,
        FindingKind::HeadFieldMismatch,
    ];

    /// The item 41 name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            FindingKind::KeptByteChanged => "kept_byte_changed",
            FindingKind::SegmentLineRemoved => "segment_line_removed",
            FindingKind::SegmentByteChanged => "segment_byte_changed",
            FindingKind::BatchHashMismatch => "batch_hash_mismatch",
            FindingKind::TornTailBeyondHead => "torn_tail_beyond_head",
            FindingKind::ForkPrevMismatch => "fork_prev_mismatch",
            FindingKind::HeadOffsetMismatch => "head_offset_mismatch",
            FindingKind::PackedSegmentCorrupt => "packed_segment_corrupt",
            FindingKind::HeadFieldMismatch => "head_field_mismatch",
        }
    }

    /// The catalog code `tm audit` reports it under: `E2101` for a torn
    /// tail, `E2102` for a fork, `E2103` for corruption.
    #[must_use]
    pub fn code(self) -> tm_formats::Code {
        match self {
            FindingKind::TornTailBeyondHead => tm_formats::Code::E2101,
            FindingKind::ForkPrevMismatch | FindingKind::HeadFieldMismatch => {
                tm_formats::Code::E2102
            }
            _ => tm_formats::Code::E2103,
        }
    }
}

impl fmt::Display for FindingKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// One finding with where it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// What.
    pub kind: FindingKind,
    /// The segment, for chain findings.
    pub segment: Option<u64>,
    /// The offset within the segment, for chain findings.
    pub offset: Option<u64>,
    /// The object's fingerprint, for `kept_byte_changed`.
    pub object: Option<EventId>,
    /// Detail for the report.
    pub message: String,
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.kind)?;
        if let Some(s) = self.segment {
            write!(f, " segment {s}")?;
        }
        if let Some(o) = self.offset {
            write!(f, " offset {o}")?;
        }
        if let Some(o) = &self.object {
            write!(f, " object {o}")?;
        }
        write!(f, ": {}", self.message)
    }
}

fn at(kind: FindingKind, segment: u64, offset: u64, message: impl Into<String>) -> Finding {
    Finding {
        kind,
        segment: Some(segment),
        offset: Some(offset),
        object: None,
        message: message.into(),
    }
}

/// The chain state carried from batch to batch.
struct Link {
    /// The previous event's ID, when the chain is intact up to here.
    prev_id: Option<EventId>,
    seq: u64,
    batch: u64,
    epoch: u64,
}

impl Record {
    /// Verify the chain, `HEAD`, and every kept object. An empty list is a
    /// clean record.
    ///
    /// # Errors
    /// `Error::Io`; `E2103` when `HEAD` or `record.json` cannot be read at
    /// all.
    pub fn verify(&self) -> Result<Vec<Finding>> {
        let main = self.main_dir();
        let head = Head::read(&main)?;
        let mut findings = Vec::new();
        let mut link = Link {
            prev_id: Some(self.genesis()),
            seq: 0,
            batch: 0,
            epoch: 0,
        };
        let mut open_clean = true;
        for n in 1..=head.segment {
            let before = findings.len();
            self.verify_segment(n, &head, &mut link, &mut findings)?;
            if n == head.segment {
                open_clean = findings.len() == before;
            }
        }
        let next = segment::forms(&main, head.segment + 1);
        if next == (true, false)
            && segment::forms(&main, head.segment) == (false, true)
            && open_clean
        {
            // 3.1: the uncommitted segment a crash after a completed sealing
            // leaves behind; the next lock removes it.
            let len = segment::plain_len(&main, head.segment + 1)?;
            findings.push(at(
                FindingKind::TornTailBeyondHead,
                head.segment + 1,
                0,
                format!(
                    "{len} bytes in segment {} after the sealed HEAD.segment {}",
                    head.segment + 1,
                    head.segment
                ),
            ));
        } else if next != (false, false) {
            findings.push(Finding {
                kind: FindingKind::HeadFieldMismatch,
                segment: Some(head.segment + 1),
                offset: None,
                object: None,
                message: format!(
                    "segment {} exists beyond HEAD.segment {}",
                    head.segment + 1,
                    head.segment
                ),
            });
        }
        if let Some(prev) = &link.prev_id {
            if *prev != head.last_id {
                findings.push(Finding {
                    kind: FindingKind::HeadFieldMismatch,
                    segment: None,
                    offset: None,
                    object: None,
                    message: format!("HEAD.last_id {} is not the last event {prev}", head.last_id),
                });
            }
            if link.seq != head.seq {
                findings.push(Finding {
                    kind: FindingKind::HeadFieldMismatch,
                    segment: None,
                    offset: None,
                    object: None,
                    message: format!(
                        "HEAD.seq {} but the last event has seq {}",
                        head.seq, link.seq
                    ),
                });
            }
            if link.batch != head.batch {
                findings.push(Finding {
                    kind: FindingKind::HeadFieldMismatch,
                    segment: None,
                    offset: None,
                    object: None,
                    message: format!(
                        "HEAD.batch {} but the last batch is {}",
                        head.batch, link.batch
                    ),
                });
            }
        }
        self.verify_objects(&mut findings)?;
        Ok(findings)
    }

    /// Load segment `n` for verification; `None` (after a finding) when it
    /// is missing or does not decode.
    fn load_for_verify(
        &self,
        n: u64,
        head: &Head,
        link: &mut Link,
        findings: &mut Vec<Finding>,
    ) -> Result<Option<segment::Loaded>> {
        match segment::load(&self.main_dir(), n) {
            Ok(Some(l)) => Ok(Some(l)),
            Ok(None) => {
                if !(n == head.segment && head.offset == 0) {
                    findings.push(at(
                        FindingKind::SegmentLineRemoved,
                        n,
                        0,
                        format!("segment {n} is missing"),
                    ));
                    link.prev_id = None;
                }
                Ok(None)
            }
            Err(LoadError::PackedCorrupt(_, why)) => {
                findings.push(at(FindingKind::PackedSegmentCorrupt, n, 0, why));
                link.prev_id = None;
                Ok(None)
            }
            Err(e) => Err(e.into()),
        }
    }

    fn verify_segment(
        &self,
        n: u64,
        head: &Head,
        link: &mut Link,
        findings: &mut Vec<Finding>,
    ) -> Result<()> {
        let Some(loaded) = self.load_for_verify(n, head, link, findings)? else {
            return Ok(());
        };
        let len = loaded.bytes.len() as u64;
        let mut end = len;
        let mut offset_checked = false;
        if n == head.segment {
            if head.offset > len {
                findings.push(at(
                    FindingKind::HeadOffsetMismatch,
                    n,
                    head.offset,
                    format!(
                        "HEAD.offset {} is beyond the segment's {len} bytes",
                        head.offset
                    ),
                ));
                offset_checked = true;
            } else {
                end = head.offset;
            }
        }
        let mut walked_to = 0u64;
        let mut broke = false;
        for frame in segment::frames(&loaded.bytes, 0, end) {
            match frame {
                Err(FrameError::BadHeader { offset }) => {
                    findings.push(at(
                        FindingKind::SegmentByteChanged,
                        n,
                        offset,
                        "not a batch header line",
                    ));
                    broke = true;
                    break;
                }
                Err(FrameError::Short {
                    offset,
                    header,
                    have,
                }) => {
                    findings.push(at(
                        FindingKind::SegmentLineRemoved,
                        n,
                        offset,
                        format!(
                            "batch {} declares {} payload bytes, {have} present",
                            header.b, header.len
                        ),
                    ));
                    broke = true;
                    break;
                }
                Ok(frame) => {
                    walked_to = frame.end;
                    verify_batch(n, &frame, link, findings);
                }
            }
        }
        if broke {
            link.prev_id = None;
        } else if n == head.segment && !offset_checked && walked_to != end {
            findings.push(at(
                FindingKind::HeadOffsetMismatch,
                n,
                head.offset,
                format!(
                    "HEAD.offset {} is inside a batch (batches end at {walked_to})",
                    head.offset
                ),
            ));
        }
        if n == head.segment && len > head.offset {
            findings.push(at(
                FindingKind::TornTailBeyondHead,
                n,
                head.offset,
                format!("{} bytes beyond HEAD.offset", len - head.offset),
            ));
        }
        Ok(())
    }

    fn verify_objects(&self, findings: &mut Vec<Finding>) -> Result<()> {
        for fp in self.objects()? {
            let path = self.object_path(&fp);
            let finding = |msg: String| Finding {
                kind: FindingKind::KeptByteChanged,
                segment: None,
                offset: None,
                object: Some(fp.clone()),
                message: msg,
            };
            let compressed = match std::fs::read(&path) {
                Ok(b) => b,
                Err(e) => return Err(crate::Error::io(&path, e)),
            };
            match zstd::decode_all(&compressed[..]) {
                Ok(bytes) => {
                    let computed = id_from_digest(&Sha256::digest(&bytes));
                    if computed != fp {
                        findings.push(finding(format!("bytes hash to {computed}")));
                    }
                }
                Err(e) => findings.push(finding(format!("does not decode: {e}"))),
            }
        }
        Ok(())
    }
}

fn verify_batch(n: u64, frame: &segment::Frame<'_>, link: &mut Link, findings: &mut Vec<Finding>) {
    let header = &frame.header;
    // 1. Line count against `n`.
    let Some(lines) = split_lines(frame.payload) else {
        findings.push(at(
            FindingKind::SegmentByteChanged,
            n,
            frame.payload_offset,
            "payload does not end with a newline",
        ));
        link.prev_id = None;
        return;
    };
    if lines.len() as u64 != header.n {
        findings.push(at(
            FindingKind::SegmentLineRemoved,
            n,
            frame.header_offset,
            format!(
                "batch {} declares {} events, {} lines present",
                header.b,
                header.n,
                lines.len()
            ),
        ));
        link.prev_id = None;
        return;
    }
    // 2. Each line's ID against its bytes.
    let mut events: Vec<StoredEvent> = Vec::with_capacity(lines.len());
    let mut offset = frame.payload_offset;
    for l in lines {
        match parse_stored(Position { segment: n, offset }, l) {
            Ok(e) => events.push(e),
            Err(e) => {
                findings.push(at(
                    FindingKind::SegmentByteChanged,
                    n,
                    offset,
                    e.to_string(),
                ));
                link.prev_id = None;
                return;
            }
        }
        offset += l.len() as u64 + 1;
    }
    // 3. The payload against `h`.
    if payload_hash(frame.payload) != header.h {
        findings.push(at(
            FindingKind::BatchHashMismatch,
            n,
            frame.header_offset,
            format!("batch {} payload does not hash to its header's h", header.b),
        ));
    }
    // 4. Links.
    if let Some((o, msg)) = check_links(frame, &events, link) {
        findings.push(at(FindingKind::ForkPrevMismatch, n, o, msg));
        link.prev_id = None;
    }
    link.batch = header.b;
    link.epoch = header.epoch;
}

/// The header against the previous header, each event against its
/// predecessor and its header; advances `link` past every event that
/// links. Returns the first failure's offset and reason.
fn check_links(
    frame: &segment::Frame<'_>,
    events: &[StoredEvent],
    link: &mut Link,
) -> Option<(u64, String)> {
    let header = &frame.header;
    if link.prev_id.is_some() {
        if header.b != link.batch + 1 {
            return Some((
                frame.header_offset,
                format!("batch {} follows batch {}", header.b, link.batch),
            ));
        }
        if header.epoch < link.epoch {
            return Some((
                frame.header_offset,
                format!("epoch {} after epoch {}", header.epoch, link.epoch),
            ));
        }
    }
    for e in events {
        let o = e.position.offset;
        if e.batch != header.b || e.epoch != header.epoch {
            return Some((
                o,
                format!(
                    "event carries batch {} epoch {} under header b {} epoch {}",
                    e.batch, e.epoch, header.b, header.epoch
                ),
            ));
        }
        if let Some(prev) = &link.prev_id {
            if e.prev().as_ref() != Some(prev) {
                return Some((
                    o,
                    format!(
                        "prev {} is not the previous event {prev}",
                        e.prev().map(|p| p.to_string()).unwrap_or_default()
                    ),
                ));
            }
            if e.seq != link.seq + 1 {
                return Some((o, format!("seq {} follows seq {}", e.seq, link.seq)));
            }
        }
        link.prev_id = Some(e.id().clone());
        link.seq = e.seq;
    }
    None
}
