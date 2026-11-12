//! The main-chain lock (item 35; 3.1; 11.21; D15): `O_EXCL` creation,
//! in-place heartbeat rewrites through the descriptor created with
//! `O_EXCL`, the epoch written to `HEAD` before anything else, takeover
//! after a stale heartbeat measured on the monotonic clock, and the torn
//! tail found and truncated at acquisition.

use std::fs::{File, OpenOptions};
use std::os::unix::fs::{FileExt, MetadataExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::head::Head;
use crate::segment::{plain_len, plain_path};
use crate::{Code, Error, Record, Result, fsync_dir, now_at, refresh_dir};

/// How to take the lock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockOptions {
    /// `lock.heartbeat_seconds` (default 5 s).
    pub heartbeat: Duration,
    /// `lock.takeover_seconds` (default 300 s, OQ2): a heartbeat unchanged
    /// for this long allows a takeover.
    pub takeover: Duration,
    /// How long to wait for a live holder before giving up with `E2201`;
    /// `None` waits as long as the holder is alive and takes over when it
    /// goes stale (an automatic takeover, 3.1).
    pub wait: Option<Duration>,
    /// How often to re-read the lock while waiting.
    pub poll: Duration,
    /// The host name to record; `None` uses `uname`'s node name.
    pub host: Option<String>,
}

impl Default for LockOptions {
    fn default() -> Self {
        LockOptions {
            heartbeat: Duration::from_secs(5),
            takeover: Duration::from_secs(300),
            wait: None,
            poll: Duration::from_secs(1),
            host: None,
        }
    }
}

/// The lock file's content: `{"host","pid","started","heartbeat","epoch"}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockHolder {
    /// Host name.
    pub host: String,
    /// Process ID.
    pub pid: u64,
    /// When the lock was taken, `at` form.
    pub started: String,
    /// The last heartbeat, `at` form.
    pub heartbeat: String,
    /// The holder's epoch.
    pub epoch: u64,
}

impl LockHolder {
    /// The JSON object.
    #[must_use]
    pub fn to_json(&self) -> Value {
        json!({
            "host": self.host,
            "pid": self.pid,
            "started": self.started,
            "heartbeat": self.heartbeat,
            "epoch": self.epoch,
        })
    }

    /// Parse; `None` for anything that is not a lock file.
    #[must_use]
    pub fn from_json(v: &Value) -> Option<LockHolder> {
        let m = v.as_object()?;
        if m.len() != 5 {
            return None;
        }
        let s = |k: &str| m.get(k)?.as_str().map(str::to_owned);
        Some(LockHolder {
            host: s("host")?,
            pid: m.get("pid")?.as_u64()?,
            started: s("started")?,
            heartbeat: s("heartbeat")?,
            epoch: m.get("epoch")?.as_u64()?,
        })
    }

    /// The `previous` object of `lock.taken_over` (11.16).
    #[must_use]
    pub fn previous_body(&self) -> Value {
        json!({
            "host": self.host,
            "pid": self.pid,
            "epoch": self.epoch,
            "heartbeat": self.heartbeat,
        })
    }

    fn malformed() -> LockHolder {
        LockHolder {
            host: String::new(),
            pid: 0,
            started: String::new(),
            heartbeat: String::new(),
            epoch: 0,
        }
    }
}

/// Read `main/lock` after a directory refresh: `None` when absent; a lock
/// file that does not parse (being written, or damaged) reads as a holder
/// with epoch 0, which no real holder has.
///
/// # Errors
/// `Error::Io` for anything but a missing file.
pub(crate) fn read_lock_file(main: &Path) -> Result<Option<LockHolder>> {
    Ok(read_lock_raw(main)?.map(|raw| parse_holder(&raw)))
}

fn parse_holder(raw: &[u8]) -> LockHolder {
    tm_formats::canonical::parse_bytes(raw)
        .ok()
        .and_then(|v| LockHolder::from_json(&v))
        .unwrap_or_else(LockHolder::malformed)
}

fn read_lock_raw(main: &Path) -> Result<Option<Vec<u8>>> {
    refresh_dir(main);
    let path = main.join("lock");
    match std::fs::read(&path) {
        Ok(raw) => Ok(Some(raw)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(Error::io(path, e)),
    }
}

/// A torn tail found and truncated at lock time (3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TornTail {
    /// The open segment.
    pub segment: u64,
    /// The offset the tail was cut at (`HEAD.offset`).
    pub offset: u64,
    /// Bytes dropped.
    pub bytes: u64,
}

impl TornTail {
    /// The `chain.torn_tail_truncated` body (11.16).
    #[must_use]
    pub fn body(&self) -> Value {
        json!({"segment": self.segment, "offset": self.offset, "bytes": self.bytes})
    }
}

/// The lock, held until dropped. Dropping stops the heartbeat and removes
/// the lock file when it is still this holder's inode.
#[derive(Debug)]
pub struct Lock {
    path: PathBuf,
    file: Arc<File>,
    epoch: u64,
    holder: LockHolder,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    torn_tail: Option<TornTail>,
    taken_over: Option<LockHolder>,
    head: Head,
}

impl Lock {
    /// This holder's epoch.
    #[must_use]
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// This holder's lock file content as first written.
    #[must_use]
    pub fn holder(&self) -> &LockHolder {
        &self.holder
    }

    /// `HEAD` as rewritten at lock time (the epoch incremented, the rest as
    /// found).
    #[must_use]
    pub fn head_at_lock(&self) -> &Head {
        &self.head
    }

    /// The torn tail truncated at acquisition, for the
    /// `chain.torn_tail_truncated` event item 75 puts first in the batch.
    #[must_use]
    pub fn torn_tail(&self) -> Option<TornTail> {
        self.torn_tail
    }

    /// The stale holder displaced by a takeover, for the `lock.taken_over`
    /// event.
    #[must_use]
    pub fn taken_over(&self) -> Option<&LockHolder> {
        self.taken_over.as_ref()
    }

    /// The `lock.taken_over` body (11.16), when this acquisition was one.
    #[must_use]
    pub fn taken_over_body(&self) -> Option<Value> {
        self.taken_over
            .as_ref()
            .map(|p| json!({"previous": p.previous_body(), "epoch": self.epoch}))
    }

    /// Rewrite the heartbeat now (the thread does this every
    /// `heartbeat`; a long step may call it too).
    ///
    /// # Errors
    /// `Error::Io`.
    pub fn refresh_heartbeat(&self) -> Result<()> {
        let mut holder = self.holder.clone();
        holder.heartbeat = now_at();
        write_in_place(&self.file, &holder).map_err(|e| Error::io(&self.path, e))
    }

    /// Stop the heartbeat thread without releasing: what a paused or
    /// stopped process looks like to other writers (item 42's simulated
    /// takeover). The lock file stays until `release` or `Drop`.
    pub fn suspend_heartbeat(&mut self) {
        self.stop_thread();
    }

    /// Release the lock now (what `Drop` does), reporting errors.
    ///
    /// # Errors
    /// `Error::Io` when the lock file cannot be removed.
    pub fn release(mut self) -> Result<()> {
        self.stop_thread();
        self.remove_if_ours().map_err(|e| Error::io(&self.path, e))
    }

    fn stop_thread(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }

    fn remove_if_ours(&self) -> std::io::Result<()> {
        let ours = self.file.metadata()?.ino();
        match std::fs::metadata(&self.path) {
            Ok(md) if md.ino() == ours => std::fs::remove_file(&self.path),
            Ok(_) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        self.stop_thread();
        let _ = self.remove_if_ours();
    }
}

fn write_in_place(file: &File, holder: &LockHolder) -> std::io::Result<()> {
    let mut text = tm_formats::canonical::canonical(&holder.to_json())
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    text.push('\n');
    file.write_all_at(text.as_bytes(), 0)?;
    file.set_len(text.len() as u64)?;
    file.sync_data()
}

fn host_name(opts: &LockOptions) -> String {
    opts.host.clone().unwrap_or_else(|| {
        rustix::system::uname()
            .nodename()
            .to_string_lossy()
            .into_owned()
    })
}

impl Record {
    /// Take the lock (3.1), waiting for a live holder and taking over a
    /// stale one. `on_wait` is called on each poll with the holder and the
    /// time until a takeover is allowed. On acquisition the epoch is
    /// incremented and written to `HEAD` before anything else, then a torn
    /// tail beyond `HEAD.offset` is truncated.
    ///
    /// # Errors
    /// `E2201` when `opts.wait` passes with the heartbeat still changing;
    /// `E2103` when the open segment is shorter than `HEAD.offset`;
    /// `Error::NewerFormat`; `Error::Io`.
    pub fn lock(
        &self,
        opts: &LockOptions,
        mut on_wait: impl FnMut(&LockHolder, Duration),
    ) -> Result<Lock> {
        self.writable()?;
        let main = self.main_dir();
        let path = main.join("lock");
        let start = Instant::now();
        let mut observed: Option<(Vec<u8>, Instant)> = None;
        let mut taken_over: Option<LockHolder> = None;
        loop {
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(file) => return self.finish_lock(file, path, opts, taken_over),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(Error::io(&path, e)),
            }
            let Some(raw) = read_lock_raw(&main)? else {
                continue;
            };
            let holder = parse_holder(&raw);
            let since = match &observed {
                Some((prev, since)) if *prev == raw => *since,
                _ => {
                    let now = Instant::now();
                    observed = Some((raw.clone(), now));
                    now
                }
            };
            let unchanged_for = since.elapsed();
            if unchanged_for >= opts.takeover {
                // Stale. Break it only if it is still exactly what we observed.
                if read_lock_raw(&main)?.as_deref() == Some(raw.as_slice()) {
                    match std::fs::remove_file(&path) {
                        Ok(()) => {}
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                        Err(e) => return Err(Error::io(&path, e)),
                    }
                    taken_over = Some(holder);
                    observed = None;
                    continue;
                }
                observed = None;
                continue;
            }
            on_wait(&holder, opts.takeover.saturating_sub(unchanged_for));
            if let Some(wait) = opts.wait
                && start.elapsed() >= wait
            {
                return Err(Error::code(
                    Code::E2201,
                    format!(
                        "lock held by {} (pid {}), heartbeat still changing; takeover allowed after {} s without one",
                        holder.host,
                        holder.pid,
                        opts.takeover.as_secs()
                    ),
                ));
            }
            std::thread::sleep(opts.poll);
        }
    }

    /// `tm lock break` (section 9): wait one takeover period and take the
    /// lock if the heartbeat did not change, else `E2201`.
    ///
    /// # Errors
    /// As [`Record::lock`].
    pub fn break_lock(
        &self,
        opts: &LockOptions,
        on_wait: impl FnMut(&LockHolder, Duration),
    ) -> Result<Lock> {
        let forced = LockOptions {
            wait: Some(opts.takeover + opts.poll),
            ..opts.clone()
        };
        self.lock(&forced, on_wait)
    }

    /// The current lock holder, if any (`tm lock status`).
    ///
    /// # Errors
    /// `Error::Io`.
    pub fn lock_status(&self) -> Result<Option<LockHolder>> {
        read_lock_file(&self.main_dir())
    }

    fn finish_lock(
        &self,
        file: File,
        path: PathBuf,
        opts: &LockOptions,
        taken_over: Option<LockHolder>,
    ) -> Result<Lock> {
        let main = self.main_dir();
        let cleanup = |e: Error| {
            let _ = std::fs::remove_file(&path);
            e
        };
        let found = Head::read(&main).map_err(cleanup)?;
        let epoch = found.epoch + 1;
        let now = now_at();
        let holder = LockHolder {
            host: host_name(opts),
            pid: u64::from(std::process::id()),
            started: now.clone(),
            heartbeat: now,
            epoch,
        };
        write_in_place(&file, &holder).map_err(|e| cleanup(Error::io(&path, e)))?;
        fsync_dir(&main).map_err(cleanup)?;
        // Lock-time HEAD write (11.21): the incremented epoch, the rest unchanged.
        let head = Head { epoch, ..found };
        head.write_checked(&main, Some(epoch)).map_err(cleanup)?;
        // Torn tail beyond HEAD.offset (3.1).
        let len = plain_len(&main, head.segment).map_err(cleanup)?;
        let torn_tail = match len.cmp(&head.offset) {
            std::cmp::Ordering::Greater => {
                let seg = plain_path(&main, head.segment);
                let f = OpenOptions::new()
                    .write(true)
                    .open(&seg)
                    .map_err(|e| cleanup(Error::io(&seg, e)))?;
                f.set_len(head.offset)
                    .map_err(|e| cleanup(Error::io(&seg, e)))?;
                f.sync_all().map_err(|e| cleanup(Error::io(&seg, e)))?;
                fsync_dir(&main).map_err(cleanup)?;
                Some(TornTail {
                    segment: head.segment,
                    offset: head.offset,
                    bytes: len - head.offset,
                })
            }
            std::cmp::Ordering::Less => {
                return Err(cleanup(Error::code(
                    Code::E2103,
                    format!(
                        "HEAD.offset {} is beyond the end of segment {} ({len} bytes): head_offset_mismatch",
                        head.offset, head.segment
                    ),
                )));
            }
            std::cmp::Ordering::Equal => None,
        };
        let file = Arc::new(file);
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let file = Arc::clone(&file);
            let stop = Arc::clone(&stop);
            let holder = holder.clone();
            let every = opts.heartbeat;
            std::thread::Builder::new()
                .name("tm-lock-heartbeat".into())
                .spawn(move || {
                    let mut last = Instant::now();
                    while !stop.load(Ordering::SeqCst) {
                        std::thread::sleep(Duration::from_millis(50).min(every));
                        if last.elapsed() >= every {
                            let mut h = holder.clone();
                            h.heartbeat = now_at();
                            let _ = write_in_place(&file, &h);
                            last = Instant::now();
                        }
                    }
                })
                .map_err(|e| cleanup(Error::io(&path, e)))?
        };
        Ok(Lock {
            path,
            file,
            epoch,
            holder,
            stop,
            thread: Some(thread),
            torn_tail,
            taken_over,
            head,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn holder_json() {
        let h = LockHolder {
            host: "node1".into(),
            pid: 42,
            started: "2026-11-11T12:00:00Z".into(),
            heartbeat: "2026-11-11T12:00:05Z".into(),
            epoch: 3,
        };
        assert_eq!(LockHolder::from_json(&h.to_json()), Some(h.clone()));
        assert_eq!(h.previous_body()["epoch"], 3);
        assert_eq!(parse_holder(b"garbage").epoch, 0);
    }
}
