//! Item 31: temp-dir cases for snapshots, the racy rule, the re-hash
//! decision, exclusions, and manifest diffs.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a test harness: panics report setup failures"
)]

use std::fs;
use std::io::Write;
use std::path::Path;
use std::time::{Duration, SystemTime};

use tm_fsfacts::hash::{HashSettings, hash_file};
use tm_fsfacts::manifest::{DEFAULT_IGNORE, Manifest};
use tm_fsfacts::needs_rehash::{Context, Decision, RehashSettings, decide};
use tm_fsfacts::racy::{Marker, is_racy};
use tm_fsfacts::snapshot::Snapshot;

fn write(path: &Path, bytes: &[u8]) {
    let mut f = fs::File::create(path).unwrap();
    f.write_all(bytes).unwrap();
    f.sync_all().unwrap();
}

fn settle() {
    // Put the next change in a later clock tick than the last one.
    std::thread::sleep(Duration::from_millis(30));
}

fn decide_run(prev: &Snapshot, cur: &Snapshot) -> Decision {
    decide(
        Some(prev),
        cur,
        false,
        &RehashSettings::default(),
        Context::Run,
    )
}

#[test]
fn unchanged_and_size_change() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("a.txt");
    write(&p, b"hello");
    let s1 = Snapshot::take(&p).unwrap();
    let s2 = Snapshot::take(&p).unwrap();
    assert_eq!(s1, s2);
    assert_eq!(decide_run(&s1, &s2), Decision::Unchanged);
    settle();
    write(&p, b"hello, world");
    let s3 = Snapshot::take(&p).unwrap();
    assert_ne!(s1.size, s3.size);
    assert_eq!(decide_run(&s1, &s3), Decision::HashNow);
}

#[test]
fn mtime_unchanged_content_change_is_caught_by_ctime() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("a.txt");
    write(&p, b"hello");
    let s1 = Snapshot::take(&p).unwrap();
    let old_mtime = fs::metadata(&p).unwrap().modified().unwrap();
    settle();
    write(&p, b"jello");
    fs::File::options()
        .write(true)
        .open(&p)
        .unwrap()
        .set_modified(old_mtime)
        .unwrap();
    let s2 = Snapshot::take(&p).unwrap();
    assert_eq!(s1.size, s2.size);
    assert_eq!(s1.mtime, s2.mtime, "mtime was restored");
    assert_ne!(s1.ctime, s2.ctime, "ctime moved");
    assert_eq!(decide_run(&s1, &s2), Decision::HashNow);
    let untrusting = RehashSettings {
        trust_ctime: false,
        ..RehashSettings::default()
    };
    assert_eq!(
        decide(Some(&s1), &s2, false, &untrusting, Context::Run),
        Decision::Unchanged
    );
}

#[test]
fn mtime_equal_to_marker_is_rehashed() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("a.txt");
    write(&p, b"hello");
    let marker = Marker::touch(dir.path()).unwrap();
    let marker_time = SystemTime::UNIX_EPOCH
        + Duration::new(
            u64::try_from(marker.mtime.secs).unwrap(),
            marker.mtime.nanos,
        );
    fs::File::options()
        .write(true)
        .open(&p)
        .unwrap()
        .set_modified(marker_time)
        .unwrap();
    let s = Snapshot::take(&p).unwrap();
    assert_eq!(s.mtime, marker.mtime);
    assert!(is_racy(&s, &marker));
    assert_eq!(
        decide(
            Some(&s),
            &s,
            is_racy(&s, &marker),
            &RehashSettings::default(),
            Context::Run
        ),
        Decision::HashNow
    );
    settle();
    let later = Marker::touch(dir.path()).unwrap();
    assert!(!is_racy(&s, &later));
    assert_eq!(
        decide(
            Some(&s),
            &s,
            is_racy(&s, &later),
            &RehashSettings::default(),
            Context::Run
        ),
        Decision::Unchanged
    );
}

#[test]
fn excluded_entries_and_manifest_diff() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("sub/.snakemake/x")).unwrap();
    fs::create_dir_all(root.join("sub/deeper")).unwrap();
    write(&root.join("b.csv"), b"1,2\n");
    write(&root.join("a.csv"), b"3,4\n");
    write(&root.join("sub/deeper/c.txt"), b"c");
    write(&root.join("sub/.snakemake/x/log"), b"log");
    write(&root.join("sub/notes.swp"), b"swap");
    write(&root.join(".DS_Store"), b"mac");
    std::os::unix::fs::symlink(root.join("a.csv"), root.join("link.csv")).unwrap();
    let ignore: Vec<String> = DEFAULT_IGNORE.iter().map(|s| (*s).to_owned()).collect();
    let settings = HashSettings {
        threads: 2,
        bandwidth_mib: 0,
    };
    let m1 = Manifest::build(root, &ignore, &settings).unwrap();
    let paths: Vec<&str> = m1.entries.iter().map(|e| e.path.as_str()).collect();
    assert_eq!(
        paths,
        ["a.csv", "b.csv", "sub/deeper/c.txt"],
        "sorted, excluded, no links"
    );
    assert_eq!(m1.size(), 4 + 4 + 1);
    assert_eq!(m1.ignore, ignore);
    assert_eq!(
        m1.entries[0].fingerprint,
        hash_file(&root.join("a.csv"), &HashSettings::default())
            .unwrap()
            .fingerprint
    );
    let json = m1.entries_json();
    assert_eq!(json[2]["path"], "sub/deeper/c.txt");
    assert_eq!(json[2]["snapshot"]["size"], 1);

    settle();
    write(&root.join("b.csv"), b"1,2,3\n");
    fs::remove_file(root.join("a.csv")).unwrap();
    write(&root.join("sub/d.txt"), b"d");
    let m2 = Manifest::build(root, &ignore, &settings).unwrap();
    let d = m2.diff(&m1);
    assert_eq!(d.added, ["sub/d.txt"]);
    assert_eq!(d.removed, ["a.csv"]);
    assert_eq!(d.modified, ["b.csv"]);
    assert!(m1.diff(&m1).is_empty());
}

#[test]
fn non_utf8_entry_is_e3003() {
    use std::os::unix::ffi::OsStrExt;
    let dir = tempfile::tempdir().unwrap();
    let bad = dir.path().join(std::ffi::OsStr::from_bytes(b"bad\xff.txt"));
    write(&bad, b"x");
    let err = Manifest::build(dir.path(), &[], &HashSettings::default()).unwrap_err();
    match err {
        tm_fsfacts::Error::Format(e) => assert_eq!(e.code, tm_formats::Code::E3003),
        other @ tm_fsfacts::Error::Io { .. } => panic!("{other}"),
    }
}
