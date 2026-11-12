//! Item 40: the cases under `conformance/record/`. Chains verify clean and
//! match their `expected.json` (events, positions, head, segments,
//! objects, lock); every item 41 tampering case is reported by its name.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_lines,
    reason = "a test harness: one pass over every case"
)]

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::Read as _;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use tm_record::head::Head;
use tm_record::segment;
use tm_record::{FindingKind, Position, Record};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance/record")
}

fn cases(kind: &str) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(root().join(kind))
        .expect("case directory")
        .map(|e| e.expect("entry").path())
        .filter(|p| p.is_dir())
        .collect();
    v.sort();
    v
}

fn expected(dir: &Path) -> Value {
    let text = std::fs::read_to_string(dir.join("expected.json")).expect("expected.json");
    serde_json::from_str(&text).expect("expected.json parses")
}

fn names(record: &Record) -> Vec<String> {
    record
        .verify()
        .expect("verify runs")
        .iter()
        .map(|f| f.kind.name().to_owned())
        .collect()
}

#[test]
fn chains_verify_clean_and_match_expected() {
    let dirs = cases("chains");
    assert!(dirs.len() >= 8, "chain cases present");
    for dir in dirs {
        let name = dir.file_name().unwrap().to_string_lossy().into_owned();
        let want = expected(&dir);
        let record = Record::open(&dir.join("record")).expect("open");
        assert_eq!(
            record.header().project_id,
            "p-9f3c2d1e4b5a",
            "{name}: record.json"
        );
        assert_eq!(
            names(&record),
            Vec::<String>::new(),
            "{name}: verifies clean"
        );

        let head = Head::read(&record.main_dir()).expect("HEAD");
        assert_eq!(head.to_json(), want["head"], "{name}: HEAD");

        let events: Vec<_> = record
            .events()
            .expect("scan")
            .map(|e| e.expect("event"))
            .collect();
        assert_eq!(
            events.len() as u64,
            want["events"].as_u64().unwrap(),
            "{name}: event count"
        );
        let positions: Vec<Value> = events
            .iter()
            .map(|e| json!([e.position.segment, e.position.offset, e.id().as_str()]))
            .collect();
        assert_eq!(
            Value::Array(positions),
            want["positions"],
            "{name}: positions and IDs"
        );
        let batches: std::collections::BTreeSet<u64> = events.iter().map(|e| e.batch).collect();
        assert_eq!(
            batches.len() as u64,
            want["batches"].as_u64().unwrap(),
            "{name}: batches"
        );
        let mut types: BTreeMap<String, u64> = BTreeMap::new();
        for e in &events {
            *types.entry(e.type_name().to_owned()).or_default() += 1;
        }
        assert_eq!(json!(types), want["types"], "{name}: types");

        // Every position reads back to the same event; the last is found by ID.
        for e in &events {
            let again = record.read_at(e.position).expect("read_at");
            assert_eq!(again.id(), e.id(), "{name}: read_at {:?}", e.position);
            assert_eq!(again.seq, e.seq);
        }
        if let Some(last) = events.last() {
            let found = record
                .find_by_id(last.id())
                .expect("find")
                .expect("present");
            assert_eq!(found.position, last.position, "{name}: find_by_id");
            assert_eq!(head.last_id, *last.id(), "{name}: HEAD.last_id");
            let after: Vec<_> = record
                .events_after(events[0].position)
                .expect("after")
                .map(|e| e.expect("event"))
                .collect();
            assert_eq!(after.len(), events.len() - 1, "{name}: events_after");
        }
        assert!(
            record
                .read_at(Position {
                    segment: head.segment,
                    offset: head.offset
                })
                .is_err(),
            "{name}: nothing at HEAD.offset"
        );

        // Segments: form and batch count.
        let segs = want["segments"].as_array().unwrap();
        assert_eq!(segs.len() as u64, head.segment, "{name}: segment count");
        for s in segs {
            let n = s["segment"].as_u64().unwrap();
            let (plain, packed) = segment::forms(&record.main_dir(), n);
            let form = match (plain, packed) {
                (true, _) => "plain",
                (false, true) => "packed",
                (false, false) => "absent",
            };
            assert_eq!(form, s["form"], "{name}: segment {n} form");
            let loaded = segment::load(&record.main_dir(), n).expect("load");
            let count = loaded.map_or(0, |l| {
                let end = if n == head.segment {
                    head.offset
                } else {
                    l.bytes.len() as u64
                };
                segment::frames(&l.bytes, 0, end).count()
            });
            assert_eq!(
                count,
                usize::try_from(s["batches"].as_u64().unwrap()).unwrap(),
                "{name}: segment {n} batches"
            );
        }

        // Objects read back to their fingerprints.
        let objects: Vec<Value> = record
            .objects()
            .expect("objects")
            .iter()
            .map(|o| Value::String(o.as_str().to_owned()))
            .collect();
        assert_eq!(
            Value::Array(objects.clone()),
            want["objects"],
            "{name}: objects"
        );
        for o in record.objects().expect("objects") {
            let mut bytes = Vec::new();
            record
                .open_object(&o)
                .expect("open")
                .read_to_end(&mut bytes)
                .expect("read");
            assert_eq!(
                tm_fsfacts::hash::fingerprint_of(&bytes),
                o,
                "{name}: object bytes"
            );
        }

        // The lock file, when the case holds one.
        let lock = record
            .lock_status()
            .expect("lock status")
            .map_or(Value::Null, |h| h.to_json());
        assert_eq!(lock, want["lock"], "{name}: lock");
    }
}

#[test]
fn tamper_cases_are_reported_by_name() {
    let dirs = cases("tamper");
    let mut seen = Vec::new();
    let mut table = String::from("case                      reported\n");
    for dir in dirs {
        let name = dir.file_name().unwrap().to_string_lossy().into_owned();
        let want = expected(&dir);
        let record = Record::open(&dir.join("record")).expect("open");
        let got = names(&record);
        assert_eq!(json!(got), want["findings"], "{name}: findings");
        assert!(
            got.contains(&name),
            "{name}: reported by its own name, got {got:?}"
        );
        let _ = writeln!(table, "{name:<25} {}", got.join(", "));
        seen.push(name);
    }
    println!("{table}");
    let item_41: Vec<&str> = FindingKind::ALL
        .iter()
        .filter(|k| **k != FindingKind::HeadFieldMismatch)
        .map(|k| k.name())
        .collect();
    let mut want = item_41.clone();
    want.sort_unstable();
    assert_eq!(seen, want, "exactly item 41's eight cases");
}

/// Item 40: the committed cases are what the generator writes today, byte
/// for byte (the CI step runs the same comparison through the example).
#[test]
fn cases_regenerate_byte_identically() {
    let tmp = tempfile::tempdir().expect("tmp");
    tm_record::gen_chain::conformance(tmp.path());
    let committed = root();
    let mut compared = 0usize;
    let mut missing = Vec::new();
    let mut differing = Vec::new();
    let walk = |dir: &Path, rel: &Path| {
        let mut stack = vec![(dir.to_path_buf(), rel.to_path_buf())];
        let mut files = Vec::new();
        while let Some((d, r)) = stack.pop() {
            for entry in std::fs::read_dir(&d).expect("read_dir") {
                let entry = entry.expect("entry");
                let path = entry.path();
                let rel = r.join(entry.file_name());
                if path.is_dir() {
                    stack.push((path, rel));
                } else {
                    files.push(rel);
                }
            }
        }
        files.sort();
        files
    };
    let generated = walk(tmp.path(), Path::new(""));
    let present = walk(&committed, Path::new(""));
    assert_eq!(generated, present, "the same set of files");
    for rel in generated {
        let a = std::fs::read(tmp.path().join(&rel)).expect("generated");
        match std::fs::read(committed.join(&rel)) {
            Ok(b) if a == b => compared += 1,
            Ok(_) => differing.push(rel),
            Err(_) => missing.push(rel),
        }
    }
    assert!(missing.is_empty(), "not committed: {missing:?}");
    assert!(
        differing.is_empty(),
        "committed bytes differ: {differing:?}"
    );
    assert!(compared >= 40, "{compared} files compared");
}
