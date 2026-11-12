//! Item 42: two processes contend; a paused holder is fenced out after a
//! simulated takeover; the same two cases on an NFS mount when
//! `TM_NFS_DIR` names one (`cargo test -p tm-record --test lock -- --ignored nfs`).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a test harness: panics report setup failures"
)]

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::json;
use tm_formats::envelope::{Actor, ActorKind, Source, SourceKind};
use tm_formats::events::{EventType, KnownEvent};
use tm_record::head::Head;
use tm_record::segment;
use tm_record::{Batch, Code, LockOptions, Record, TornTail};

const AT: &str = "2026-11-11T12:00:00Z";

fn opts(takeover_ms: u64, wait_ms: Option<u64>) -> LockOptions {
    LockOptions {
        heartbeat: Duration::from_millis(100),
        takeover: Duration::from_millis(takeover_ms),
        wait: wait_ms.map(Duration::from_millis),
        poll: Duration::from_millis(20),
        host: None,
    }
}

fn event(i: u64) -> KnownEvent {
    KnownEvent::new(
        EventType::SettingsChanged,
        &json!({"key": "view.rows_per_page", "value": {"t": "int", "v": i.to_string()}, "previous": null}),
    )
    .expect("event")
}

fn batch(lock: &tm_record::Lock, i: u64) -> Batch {
    let actor = Actor {
        kind: ActorKind::Owner,
        id: "tim".into(),
    };
    let src = Source {
        source: SourceKind::Cli,
        tty: false,
        harness_env: false,
        confirmed_by: None,
    };
    let mut events = Vec::new();
    if let Some(t) = lock.torn_tail() {
        events.push(KnownEvent::new(EventType::ChainTornTailTruncated, &t.body()).expect("torn"));
    }
    if let Some(b) = lock.taken_over_body() {
        events.push(KnownEvent::new(EventType::LockTakenOver, &b).expect("takeover"));
    }
    events.push(event(i));
    Batch::build(
        lock.head_at_lock(),
        lock.epoch(),
        AT,
        &actor,
        Some(&src),
        &events,
    )
    .expect("batch")
}

fn new_record(dir: &Path) -> Record {
    Record::create(dir, "p-lock-test", "o-1", AT).expect("create")
}

/// The child process: hold the lock for `TM_LOCK_HOLD_MS`, heartbeating,
/// then release. Signals readiness by creating `<dir>/held`.
#[test]
fn child_holder() {
    let Some(dir) = std::env::var_os("TM_LOCK_CHILD_DIR") else {
        return;
    };
    let dir = PathBuf::from(dir);
    let hold: u64 = std::env::var("TM_LOCK_HOLD_MS")
        .expect("hold")
        .parse()
        .expect("ms");
    let record = Record::open(&dir).expect("open");
    let lock = record
        .lock(&opts(2000, Some(2000)), |_, _| {})
        .expect("child lock");
    std::fs::write(dir.join("held"), b"1").expect("held marker");
    std::thread::sleep(Duration::from_millis(hold));
    let head = record
        .append(&lock, &batch(&lock, 1))
        .expect("child append");
    std::fs::write(dir.join("appended"), head.seq.to_string()).expect("appended marker");
    lock.release().expect("release");
}

fn spawn_child(dir: &Path, hold_ms: u64) -> std::process::Child {
    Command::new(std::env::current_exe().expect("exe"))
        .args(["--exact", "child_holder", "--nocapture", "--test-threads=1"])
        .env("TM_LOCK_CHILD_DIR", dir)
        .env("TM_LOCK_HOLD_MS", hold_ms.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn child")
}

fn wait_for(path: &Path, limit: Duration) {
    let start = Instant::now();
    while !path.exists() {
        assert!(start.elapsed() < limit, "{} never appeared", path.display());
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn two_processes_contend(dir: &Path) {
    let record = new_record(dir);
    let mut child = spawn_child(dir, 1200);
    wait_for(&dir.join("held"), Duration::from_secs(30));
    // A live holder: waiting out `wait` fails with E2201 and the holder is
    // reported on each poll with the time until takeover.
    let mut polls = 0;
    let err = record
        .lock(&opts(5000, Some(400)), |holder, remaining| {
            polls += 1;
            assert_eq!(holder.pid, u64::from(child.id()));
            assert!(remaining <= Duration::from_millis(5000));
        })
        .expect_err("live holder keeps the lock");
    assert_eq!(err.catalog_code(), Some(Code::E2201));
    assert!(polls > 0);
    assert!(record.lock_status().expect("status").is_some());
    // When the child releases, the lock is taken without a takeover.
    let lock = record
        .lock(&opts(5000, Some(10_000)), |_, _| {})
        .expect("lock after release");
    assert!(lock.taken_over().is_none());
    assert!(child.wait().expect("child exit").success());
    wait_for(&dir.join("appended"), Duration::from_secs(5));
    assert_eq!(
        lock.head_at_lock().seq,
        1,
        "the child's batch is on the chain"
    );
    assert_eq!(lock.epoch(), 2);
    record.append(&lock, &batch(&lock, 2)).expect("append");
    lock.release().expect("release");
    assert!(record.lock_status().expect("status").is_none());
    assert_eq!(record.events().expect("scan").count(), 2);
    assert_eq!(record.verify().expect("verify"), vec![]);
}

fn paused_holder_is_fenced_out(dir: &Path) {
    let record = new_record(dir);
    let mut paused = record
        .lock(&opts(300, Some(1000)), |_, _| {})
        .expect("first lock");
    assert_eq!(paused.epoch(), 1);
    // Simulate the pause: the holder stops heartbeating (a stopped process)
    // while a second handle waits out the takeover timeout and takes over.
    paused.suspend_heartbeat();
    let other = Record::open(dir).expect("open");
    let mut saw_holder = false;
    let taker = other
        .break_lock(&opts(300, None), |holder, _| {
            saw_holder |= holder.pid == u64::from(std::process::id());
        })
        .expect("takeover");
    assert!(saw_holder, "the stale holder was reported while waiting");
    assert_eq!(taker.taken_over().map(|h| h.epoch), Some(1));
    assert!(
        !dir.join("main").join("lock.stale").exists(),
        "the stale file renamed aside is removed once the taker's lock exists"
    );
    assert_eq!(taker.epoch(), 2);
    other
        .append(&taker, &batch(&taker, 1))
        .expect("append after takeover");
    // The paused writer wakes and tries to append with its epoch-1 lock and
    // a batch built on the HEAD it saw: the fencing re-read refuses it.
    let stale_head = tm_record::Head {
        epoch: 1,
        ..tm_record::Head::initial(record.genesis())
    };
    let old_batch =
        Batch::build(&stale_head, 1, AT, &actor(), Some(&src()), &[event(9)]).expect("batch");
    let err = record.append(&paused, &old_batch).expect_err("fenced out");
    assert_eq!(err.catalog_code(), Some(Code::E2102), "{err}");
    // Releasing the paused lock must not remove the new holder's file.
    paused.release().expect("release paused");
    assert!(
        record.lock_status().expect("status").is_some(),
        "the taker's lock file survives"
    );
    taker.release().expect("release");
    assert!(record.lock_status().expect("status").is_none());
    let events: Vec<_> = record
        .events()
        .expect("scan")
        .map(|e| e.expect("event"))
        .collect();
    assert_eq!(events.len(), 2, "takeover event plus one");
    assert_eq!(events[0].type_name(), "lock.taken_over");
    assert_eq!(record.verify().expect("verify"), vec![]);
}

fn actor() -> Actor {
    Actor {
        kind: ActorKind::Owner,
        id: "tim".into(),
    }
}

fn src() -> Source {
    Source {
        source: SourceKind::Cli,
        tty: false,
        harness_env: false,
        confirmed_by: None,
    }
}

/// One batch under a fresh lock, as the Core does per command.
fn append_one(record: &Record, i: u64) -> Head {
    let lock = record
        .lock(&opts(300, Some(1000)), |_, _| {})
        .expect("lock");
    let head = record.append(&lock, &batch(&lock, i)).expect("append");
    lock.release().expect("release");
    head
}

/// 3.1: a sealing completed, then the writer died before its batch's HEAD
/// rename. The lock succeeds, the orphan segment is removed and recorded,
/// and the next append rolls to the next segment without sealing again.
fn crash_after_seal(dir: &Path) {
    let record = new_record(dir).with_segment_limit(1200);
    let main = record.main_dir();
    let mut before = Head::read(&main).expect("HEAD");
    let mut i = 1;
    loop {
        let head = append_one(&record, i);
        if head.segment > before.segment {
            break;
        }
        before = head;
        i += 1;
    }
    // Put HEAD back to the state before the batch that rolled; the epoch is
    // the one the dying writer held.
    let now = Head::read(&main).expect("HEAD");
    Head {
        epoch: now.epoch,
        ..before.clone()
    }
    .write(&main)
    .expect("HEAD rewritten");
    let sealed = before.segment;
    assert_eq!(
        segment::forms(&main, sealed),
        (false, true),
        "HEAD.segment packed-only"
    );
    assert_eq!(
        segment::forms(&main, sealed + 1),
        (true, false),
        "orphan plain segment"
    );
    let orphan_len = segment::plain_len(&main, sealed + 1).expect("len");
    assert!(orphan_len > 0);
    let names: Vec<String> = record
        .verify()
        .expect("verify")
        .iter()
        .map(|f| f.kind.name().to_owned())
        .collect();
    assert_eq!(names, ["torn_tail_beyond_head"], "audit names the state");

    let lock = record
        .lock(&opts(300, Some(1000)), |_, _| {})
        .expect("the lock succeeds after a crash between the seal and the HEAD rename");
    assert_eq!(
        lock.torn_tail(),
        Some(TornTail {
            segment: sealed + 1,
            offset: 0,
            bytes: orphan_len
        })
    );
    assert_eq!(
        segment::forms(&main, sealed + 1),
        (false, false),
        "the orphan is gone"
    );
    let b = batch(&lock, 99);
    let head = record.append(&lock, &b).expect("append rolls");
    assert_eq!(head.segment, sealed + 1);
    assert_eq!(head.offset, b.framed_len());
    assert_eq!(
        segment::forms(&main, sealed),
        (false, true),
        "not sealed again"
    );
    lock.release().expect("release");
    assert_eq!(record.verify().expect("verify"), vec![]);
    let truncations = record
        .events()
        .expect("scan")
        .map(|e| e.expect("event"))
        .filter(|e| e.type_name() == "chain.torn_tail_truncated")
        .count();
    assert_eq!(truncations, 1);
}

#[test]
fn crash_after_seal_local() {
    let dir = tempfile::tempdir().expect("tmp");
    crash_after_seal(dir.path());
}

#[test]
fn two_processes_contend_local() {
    let dir = tempfile::tempdir().expect("tmp");
    two_processes_contend(dir.path());
}

#[test]
fn paused_holder_is_fenced_out_local() {
    let dir = tempfile::tempdir().expect("tmp");
    paused_holder_is_fenced_out(dir.path());
}

fn nfs_dir() -> PathBuf {
    let base = std::env::var_os("TM_NFS_DIR")
        .expect("TM_NFS_DIR names an NFS mount for the ignored nfs cases");
    let dir = PathBuf::from(base).join(format!("tm-lock-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("mkdir on nfs");
    dir
}

#[test]
#[ignore = "needs TM_NFS_DIR on the CI NFS loopback mount"]
fn two_processes_contend_nfs() {
    two_processes_contend(&nfs_dir().join("contend"));
}

#[test]
#[ignore = "needs TM_NFS_DIR on the CI NFS loopback mount"]
fn paused_holder_is_fenced_out_nfs() {
    paused_holder_is_fenced_out(&nfs_dir().join("fenced"));
}
