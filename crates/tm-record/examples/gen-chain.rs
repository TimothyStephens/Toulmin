//! Item 40: `gen-chain --events N --out DIR` writes an N-event main chain
//! for `bench-index.sh` and item 82; `gen-chain --conformance DIR`
//! regenerates the item 41 cases under `DIR` (`conformance/record/`) with a
//! small segment limit so sealed and packed segments fit in fixtures.
//!
//! Everything the conformance mode writes is deterministic: fixed times,
//! IDs, bodies, and lock contents, so two runs produce identical bytes.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_lines,
    reason = "a generator: failures are reported by panicking"
)]

use std::collections::BTreeMap;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tm_formats::envelope::{Actor, ActorKind, Source, SourceKind};
use tm_formats::events::{EventType, KnownEvent};
use tm_formats::id::EventId;
use tm_record::head::Head;
use tm_record::segment::{self, BatchHeader, payload_hash};
use tm_record::{Batch, LockOptions, Record};

const AT: &str = "2026-11-11T12:00:00Z";
const PROJECT: &str = "p-9f3c2d1e4b5a";
const OWNER: &str = "o-7b1a0c9d";
const SMALL_LIMIT: u64 = 2500;

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

fn ev(ty: EventType, body: &Value) -> KnownEvent {
    KnownEvent::new(ty, body).unwrap_or_else(|e| panic!("{}: {e}", ty.name()))
}

fn project_created() -> KnownEvent {
    let root_hash = tm_fsfacts::hash::fingerprint_of(b"/home/tim/paper");
    ev(
        EventType::ProjectCreated,
        &json!({"project_id": PROJECT, "owner_id": OWNER, "record_format": 1, "repo_root_hash": root_hash.as_str()}),
    )
}

fn settings_changed(i: u64) -> KnownEvent {
    ev(
        EventType::SettingsChanged,
        &json!({"key": "view.rows_per_page", "value": {"t": "int", "v": (500 + i).to_string()}, "previous": null}),
    )
}

fn file_registered(n: u64) -> KnownEvent {
    ev(
        EventType::FileRegistered,
        &json!({"file": format!("F{n}"), "kind": "input", "path": format!("data/table{n}.csv"),
               "title": format!("Table {n}"), "description": "A registered input table", "source": null}),
    )
}

fn lock_options() -> LockOptions {
    LockOptions {
        heartbeat: Duration::from_secs(1),
        takeover: Duration::from_millis(300),
        wait: Some(Duration::from_secs(10)),
        poll: Duration::from_millis(50),
        host: Some("gen".into()),
    }
}

/// Take the lock, put the lock-time events first, append, release.
fn append(record: &Record, events: &[KnownEvent]) -> Head {
    let lock = record.lock(&lock_options(), |_, _| {}).expect("lock");
    let mut all = Vec::new();
    if let Some(t) = lock.torn_tail() {
        all.push(ev(EventType::ChainTornTailTruncated, &t.body()));
    }
    if let Some(b) = lock.taken_over_body() {
        all.push(ev(EventType::LockTakenOver, &b));
    }
    all.extend(events.iter().cloned());
    let batch = Batch::build(
        lock.head_at_lock(),
        lock.epoch(),
        AT,
        &actor(),
        Some(&src()),
        &all,
    )
    .expect("batch");
    let head = record.append(&lock, &batch).expect("append");
    lock.release().expect("release");
    head
}

fn new_record(dir: &Path, limit: Option<u64>) -> Record {
    let r = Record::create(dir, PROJECT, OWNER, AT).expect("create");
    match limit {
        Some(l) => r.with_segment_limit(l),
        None => r,
    }
}

fn base_three_batches(dir: &Path, limit: Option<u64>) -> Record {
    let r = new_record(dir, limit);
    append(
        &r,
        &[project_created(), settings_changed(1), file_registered(1)],
    );
    append(
        &r,
        &[settings_changed(2), file_registered(2), settings_changed(3)],
    );
    append(&r, &[settings_changed(4)]);
    r
}

fn keep_bytes(record: &Record, bytes: &[u8]) -> EventId {
    let fp = tm_fsfacts::hash::fingerprint_of(bytes);
    record.keep(&mut &bytes[..], &fp).expect("keep");
    fp
}

fn write_json(path: &Path, v: &Value) {
    let mut s = serde_json::to_string_pretty(v).unwrap();
    s.push('\n');
    fs::write(path, s).unwrap();
}

/// What a chain case freezes.
fn describe_chain(record: &Record, description: &str) -> Value {
    let head = Head::read(&record.main_dir()).unwrap();
    let mut positions = Vec::new();
    let mut types: BTreeMap<String, u64> = BTreeMap::new();
    let mut batches = std::collections::BTreeSet::new();
    for e in record.events().unwrap() {
        let e = e.unwrap();
        positions.push(json!([
            e.position.segment,
            e.position.offset,
            e.id().as_str()
        ]));
        *types.entry(e.type_name().to_owned()).or_default() += 1;
        batches.insert(e.batch);
    }
    let mut segments = Vec::new();
    for n in 1..=head.segment {
        let (plain, packed) = segment::forms(&record.main_dir(), n);
        let form = match (plain, packed) {
            (true, _) => "plain",
            (false, true) => "packed",
            (false, false) => "absent",
        };
        let count = segment::load(&record.main_dir(), n)
            .unwrap()
            .map_or(0, |l| {
                let end = if n == head.segment {
                    head.offset
                } else {
                    l.bytes.len() as u64
                };
                segment::frames(&l.bytes, 0, end).count()
            });
        segments.push(json!({"segment": n, "form": form, "batches": count}));
    }
    let objects: Vec<Value> = record
        .objects()
        .unwrap()
        .iter()
        .map(|o| Value::String(o.as_str().to_owned()))
        .collect();
    let lock = record
        .lock_status()
        .unwrap()
        .map_or(Value::Null, |h| h.to_json());
    let verify: Vec<String> = record
        .verify()
        .unwrap()
        .iter()
        .map(|f| f.kind.name().to_owned())
        .collect();
    json!({
        "description": description,
        "events": positions.len(),
        "batches": batches.len(),
        "segments": segments,
        "head": head.to_json(),
        "positions": positions,
        "types": types,
        "objects": objects,
        "lock": lock,
        "findings": verify,
    })
}

fn chain_case(root: &Path, name: &str, description: &str, build: impl FnOnce(&Path) -> Record) {
    let dir = root.join("chains").join(name);
    fs::create_dir_all(&dir).unwrap();
    let record = build(&dir.join("record"));
    let expected = describe_chain(&record, description);
    assert_eq!(
        expected["findings"],
        json!([]),
        "chain case {name} must verify clean"
    );
    write_json(&dir.join("expected.json"), &expected);
}

fn tamper_case(
    root: &Path,
    name: &str,
    description: &str,
    build: impl FnOnce(&Path) -> Record,
    tamper: impl FnOnce(&Record),
) {
    let dir = root.join("tamper").join(name);
    fs::create_dir_all(&dir).unwrap();
    let record = build(&dir.join("record"));
    tamper(&record);
    let findings: Vec<String> = record
        .verify()
        .unwrap()
        .iter()
        .map(|f| f.kind.name().to_owned())
        .collect();
    assert_eq!(
        findings,
        vec![name.to_owned()],
        "tamper case {name} reports its own name once"
    );
    write_json(
        &dir.join("expected.json"),
        &json!({"description": description, "findings": findings}),
    );
}

/// The open segment's path, bytes, and `HEAD`.
fn open_segment_frames(record: &Record) -> (PathBuf, Vec<u8>, Head) {
    let head = Head::read(&record.main_dir()).unwrap();
    let path = segment::plain_path(&record.main_dir(), head.segment);
    let bytes = fs::read(&path).unwrap();
    (path, bytes, head)
}

/// Replace the bytes of batch `b` (1-based within the open segment) with
/// `new`, shifting what follows; `HEAD.offset` is adjusted.
fn replace_batch(record: &Record, b: u64, new: &[u8], fix_head: impl FnOnce(&mut Head)) {
    let (path, bytes, mut head) = open_segment_frames(record);
    let frames: Vec<_> = segment::frames(&bytes, 0, head.offset)
        .map(|f| f.unwrap())
        .collect();
    let f = frames
        .iter()
        .find(|f| f.header.b == b)
        .expect("batch present");
    let (start, end) = (
        usize::try_from(f.header_offset).unwrap(),
        usize::try_from(f.end).unwrap(),
    );
    let mut out = bytes[..start].to_vec();
    out.extend_from_slice(new);
    out.extend_from_slice(&bytes[end..]);
    fs::write(&path, &out).unwrap();
    head.offset = out.len() as u64;
    fix_head(&mut head);
    head.write(&record.main_dir()).unwrap();
}

fn frame_bytes(header: &BatchHeader, payload: &[u8]) -> Vec<u8> {
    let mut v = header.line().into_bytes();
    v.extend_from_slice(payload);
    v
}

fn conformance(root: &Path) {
    for sub in ["chains", "tamper"] {
        let d = root.join(sub);
        if d.exists() {
            fs::remove_dir_all(&d).unwrap();
        }
        fs::create_dir_all(&d).unwrap();
    }

    // Chains ----------------------------------------------------------
    chain_case(
        root,
        "empty",
        "a record just created: the initial HEAD and no events",
        |d| new_record(d, None),
    );
    chain_case(
        root,
        "single_batch",
        "one batch of three events in one plain segment",
        |d| {
            let r = new_record(d, None);
            append(
                &r,
                &[project_created(), settings_changed(1), file_registered(1)],
            );
            r
        },
    );
    chain_case(
        root,
        "three_batches",
        "three batches in one plain segment, with the lock taken three times",
        |d| base_three_batches(d, None),
    );
    chain_case(
        root,
        "multi_segment",
        "a small segment limit: sealed segments are packed, the open one is plain",
        |d| {
            let r = new_record(d, Some(SMALL_LIMIT));
            append(&r, &[project_created(), file_registered(1)]);
            for i in 1..=4 {
                append(&r, &[settings_changed(i), file_registered(i + 1)]);
            }
            r
        },
    );
    chain_case(
        root,
        "large_batch",
        "a batch larger than the limit is a segment of its own",
        |d| {
            let r = new_record(d, Some(SMALL_LIMIT));
            append(&r, &[project_created()]);
            append(
                &r,
                &[
                    settings_changed(1),
                    settings_changed(2),
                    settings_changed(3),
                    settings_changed(4),
                    settings_changed(5),
                    settings_changed(6),
                ],
            );
            append(&r, &[file_registered(1)]);
            r
        },
    );
    chain_case(
        root,
        "objects",
        "two kept objects beside a one-batch chain",
        |d| {
            let r = new_record(d, None);
            append(
                &r,
                &[project_created(), file_registered(1), file_registered(2)],
            );
            keep_bytes(&r, b"alpha,beta\n1,2\n");
            keep_bytes(&r, &b"0123456789abcdef".repeat(4096));
            r
        },
    );
    chain_case(
        root,
        "torn_tail_truncated",
        "a torn tail beyond HEAD.offset was truncated at lock time and recorded",
        |d| {
            let r = new_record(d, None);
            append(&r, &[project_created(), file_registered(1)]);
            let (path, _, _) = open_segment_frames(&r);
            let mut f = fs::OpenOptions::new().append(true).open(&path).unwrap();
            let torn = BatchHeader {
                b: 2,
                n: 3,
                len: 900,
                h: payload_hash(b"never written"),
                epoch: 1,
            };
            f.write_all(torn.line().as_bytes()).unwrap();
            f.write_all(b"{\"partial\":").unwrap();
            drop(f);
            append(&r, &[settings_changed(1)]);
            r
        },
    );
    chain_case(
        root,
        "takeover",
        "a stale lock was taken over; the new holder's batch records it",
        |d| {
            let r = new_record(d, None);
            append(&r, &[project_created()]);
            let stale = json!({"host": "node9", "pid": 4242, "started": "2026-11-11T11:00:00Z",
                           "heartbeat": "2026-11-11T11:00:05Z", "epoch": 1});
            let text = tm_formats::canonical::canonical(&stale).unwrap() + "\n";
            fs::write(r.main_dir().join("lock"), text).unwrap();
            append(&r, &[settings_changed(1)]);
            r
        },
    );
    chain_case(
        root,
        "locked",
        "a record whose lock is held (the lock file's form)",
        |d| {
            let r = new_record(d, None);
            append(&r, &[project_created()]);
            let holder = json!({"host": "login1", "pid": 31337, "started": "2026-11-11T12:00:00Z",
                            "heartbeat": "2026-11-11T12:00:05Z", "epoch": 2});
            let text = tm_formats::canonical::canonical(&holder).unwrap() + "\n";
            fs::write(r.main_dir().join("lock"), text).unwrap();
            r
        },
    );

    // Tampering ------------------------------------------------------
    tamper_case(
        root,
        "segment_line_removed",
        "the second event line of batch 2 removed, the header's len adjusted",
        |d| base_three_batches(d, None),
        |r| {
            let (_, bytes, head) = open_segment_frames(r);
            let f = segment::frames(&bytes, 0, head.offset)
                .map(|f| f.unwrap())
                .find(|f| f.header.b == 2)
                .unwrap();
            let lines = segment::split_lines(f.payload).unwrap();
            let mut payload = Vec::new();
            for (i, l) in lines.iter().enumerate() {
                if i != 1 {
                    payload.extend_from_slice(l);
                    payload.push(b'\n');
                }
            }
            let header = BatchHeader {
                len: payload.len() as u64,
                ..f.header.clone()
            };
            replace_batch(r, 2, &frame_bytes(&header, &payload), |_| {});
        },
    );
    tamper_case(
        root,
        "segment_byte_changed",
        "one byte of an event line in batch 2 changed (the year in `at`)",
        |d| base_three_batches(d, None),
        |r| {
            let (_, bytes, head) = open_segment_frames(r);
            let f = segment::frames(&bytes, 0, head.offset)
                .map(|f| f.unwrap())
                .find(|f| f.header.b == 2)
                .unwrap();
            let text = String::from_utf8(f.payload.to_vec()).unwrap();
            let changed = text.replacen("\"at\":\"2026-", "\"at\":\"2027-", 1);
            assert_ne!(changed, text);
            replace_batch(r, 2, &frame_bytes(&f.header, changed.as_bytes()), |_| {});
        },
    );
    tamper_case(
        root,
        "batch_hash_mismatch",
        "the last hex digit of batch 2's `h` changed, payload intact",
        |d| base_three_batches(d, None),
        |r| {
            let (_, bytes, head) = open_segment_frames(r);
            let f = segment::frames(&bytes, 0, head.offset)
                .map(|f| f.unwrap())
                .find(|f| f.header.b == 2)
                .unwrap();
            let mut hex = f.header.h.as_str().to_owned();
            let last = hex.pop().unwrap();
            hex.push(if last == '0' { '1' } else { '0' });
            let header = BatchHeader {
                h: EventId::parse(&hex).unwrap(),
                ..f.header.clone()
            };
            replace_batch(r, 2, &frame_bytes(&header, f.payload), |_| {});
        },
    );
    tamper_case(
        root,
        "fork_prev_mismatch",
        "batch 3 rewritten consistently but its first event's prev is the genesis string",
        |d| base_three_batches(d, None),
        |r| {
            let (_, bytes, head) = open_segment_frames(r);
            let f = segment::frames(&bytes, 0, head.offset)
                .map(|f| f.unwrap())
                .find(|f| f.header.b == 3)
                .unwrap();
            let forged = Head {
                seq: head.seq - f.header.n,
                last_id: r.genesis(),
                batch: 2,
                ..head.clone()
            };
            let batch = Batch::build(
                &forged,
                f.header.epoch,
                AT,
                &actor(),
                Some(&src()),
                &[settings_changed(4)],
            )
            .unwrap();
            let last = batch.ids().last().unwrap().clone();
            let new = frame_bytes(&batch.header(), batch.payload());
            replace_batch(r, 3, &new, |h| h.last_id = last);
        },
    );
    tamper_case(
        root,
        "torn_tail_beyond_head",
        "a header claiming 500 payload bytes and 20 bytes of payload after HEAD.offset",
        |d| base_three_batches(d, None),
        |r| {
            let (path, _, _) = open_segment_frames(r);
            let mut f = fs::OpenOptions::new().append(true).open(&path).unwrap();
            let torn = BatchHeader {
                b: 4,
                n: 2,
                len: 500,
                h: payload_hash(b"x"),
                epoch: 3,
            };
            f.write_all(torn.line().as_bytes()).unwrap();
            f.write_all(b"{\"env\":1,\"type\":\"se").unwrap();
        },
    );
    tamper_case(
        root,
        "head_offset_mismatch",
        "HEAD.offset five bytes beyond the end of the open segment",
        |d| base_three_batches(d, None),
        |r| {
            let mut head = Head::read(&r.main_dir()).unwrap();
            head.offset += 5;
            head.write(&r.main_dir()).unwrap();
        },
    );
    tamper_case(
        root,
        "packed_segment_corrupt",
        "a byte flipped in the middle of a packed segment",
        |d| {
            let r = new_record(d, Some(SMALL_LIMIT));
            append(&r, &[project_created(), file_registered(1)]);
            for i in 1..=4 {
                append(&r, &[settings_changed(i), file_registered(i + 1)]);
            }
            r
        },
        |r| {
            let path = segment::packed_path(&r.main_dir(), 1);
            let mut c = fs::read(&path).unwrap();
            let mid = c.len() / 2;
            c[mid] ^= 0x55;
            fs::write(&path, &c).unwrap();
        },
    );
    tamper_case(
        root,
        "kept_byte_changed",
        "a kept object's file replaced by the compression of other bytes",
        |d| {
            let r = new_record(d, None);
            append(&r, &[project_created(), file_registered(1)]);
            keep_bytes(&r, b"alpha,beta\n1,2\n");
            r
        },
        |r| {
            let fp = tm_fsfacts::hash::fingerprint_of(b"alpha,beta\n1,2\n");
            let path = r.object_path(&fp);
            let other = zstd::encode_all(&b"alpha,beta\n1,3\n"[..], 3).unwrap();
            fs::write(&path, other).unwrap();
        },
    );

    let names: Vec<_> = fs::read_dir(root.join("tamper"))
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    println!(
        "wrote {} chain cases and {} tamper cases under {}",
        fs::read_dir(root.join("chains")).unwrap().count(),
        names.len(),
        root.display()
    );
}

fn bench(events: u64, out: &Path) {
    let start = Instant::now();
    let record = new_record(out, None);
    let mut written = 0u64;
    let mut i = 0u64;
    append(&record, &[project_created()]);
    written += 1;
    while written < events {
        let n = (events - written).min(100);
        let batch: Vec<KnownEvent> = (0..n)
            .map(|k| {
                i += 1;
                if k % 10 == 0 {
                    file_registered(i)
                } else {
                    settings_changed(i)
                }
            })
            .collect();
        append(&record, &batch);
        written += n;
    }
    let head = Head::read(&record.main_dir()).unwrap();
    println!(
        "gen-chain: {written} events in {} batches, {} segments, {:.1} s",
        head.batch,
        head.segment,
        start.elapsed().as_secs_f64()
    );
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut events = 1_000_000u64;
    let mut out: Option<PathBuf> = None;
    let mut conf: Option<PathBuf> = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--events" => events = it.next().expect("--events N").parse().expect("a number"),
            "--out" => out = Some(PathBuf::from(it.next().expect("--out DIR"))),
            "--conformance" => conf = Some(PathBuf::from(it.next().expect("--conformance DIR"))),
            other => {
                panic!("unknown argument {other}; use --events N --out DIR or --conformance DIR")
            }
        }
    }
    match (out, conf) {
        (Some(out), None) => bench(events, &out),
        (None, Some(root)) => conformance(&root),
        _ => panic!("use --events N --out DIR, or --conformance DIR"),
    }
}
