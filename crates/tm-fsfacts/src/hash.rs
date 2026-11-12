//! Streaming SHA-256 with the `sha256:` prefix, a thread count for hashing
//! several files, a bandwidth cap, and timing (item 27; D19).

use std::fmt::Write as _;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};
use tm_formats::id::{EventId, PREFIX};

use crate::Error;

/// A `sha256:` digest of bytes: a file's or a manifest's fingerprint
/// (glossary). The text form is the one `EventId` carries.
pub type Fingerprint = EventId;

/// The hashing settings of 11.19.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HashSettings {
    /// `hashing.threads`: files hashed at once (1 to 256).
    pub threads: usize,
    /// `hashing.bandwidth_mib`: MiB per second per thread, 0 for no cap.
    pub bandwidth_mib: u64,
}

impl Default for HashSettings {
    fn default() -> Self {
        HashSettings {
            threads: 1,
            bandwidth_mib: 0,
        }
    }
}

/// The result of hashing one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hashed {
    /// `sha256:` of the bytes.
    pub fingerprint: Fingerprint,
    /// Bytes read.
    pub bytes: u64,
    /// Wall time spent, including any throttling sleep.
    pub elapsed: Duration,
}

/// The fingerprint of in-memory bytes.
#[must_use]
pub fn fingerprint_of(bytes: &[u8]) -> Fingerprint {
    finish(Sha256::new().chain_update(bytes))
}

fn finish(h: Sha256) -> Fingerprint {
    let digest = h.finalize();
    let mut s = String::with_capacity(PREFIX.len() + 64);
    s.push_str(PREFIX);
    for b in digest {
        let _ = write!(s, "{b:02x}");
    }
    EventId::parse(&s).unwrap_or_else(|| unreachable!("a hex digest is an ID"))
}

/// Hash a reader to its end, throttled to `bandwidth_mib` when non-zero.
///
/// # Errors
/// Read errors, as `Error::Io` at `path` (the caller's name for the reader).
pub fn hash_reader(
    mut r: impl Read,
    path: &Path,
    settings: &HashSettings,
) -> Result<Hashed, Error> {
    let start = Instant::now();
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    let mut bytes = 0u64;
    loop {
        let n = r.read(&mut buf).map_err(|e| Error::io(path, e))?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
        bytes += n as u64;
        if settings.bandwidth_mib > 0 {
            // Sleep until the bytes so far fit under the cap.
            #[allow(clippy::cast_precision_loss, reason = "a rate, not an exact count")]
            let allowed = Duration::from_secs_f64(
                bytes as f64 / (settings.bandwidth_mib as f64 * 1_048_576.0),
            );
            thread::sleep(allowed.saturating_sub(start.elapsed()));
        }
    }
    Ok(Hashed {
        fingerprint: finish(h),
        bytes,
        elapsed: start.elapsed(),
    })
}

/// Hash one file.
///
/// # Errors
/// `Error::Io` when the file cannot be opened or read.
pub fn hash_file(path: &Path, settings: &HashSettings) -> Result<Hashed, Error> {
    let file = File::open(path).map_err(|e| Error::io(path, e))?;
    hash_reader(file, path, settings)
}

/// Hash several files with up to `settings.threads` threads; results are in
/// the input's order.
#[must_use]
pub fn hash_files(paths: &[PathBuf], settings: &HashSettings) -> Vec<Result<Hashed, Error>> {
    let threads = settings.threads.clamp(1, 256).min(paths.len().max(1));
    if threads == 1 {
        return paths.iter().map(|p| hash_file(p, settings)).collect();
    }
    let next = std::sync::atomic::AtomicUsize::new(0);
    let mut results: Vec<Option<Result<Hashed, Error>>> = (0..paths.len()).map(|_| None).collect();
    let slots = std::sync::Mutex::new(&mut results);
    thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if i >= paths.len() {
                        break;
                    }
                    let r = hash_file(&paths[i], settings);
                    if let Ok(mut guard) = slots.lock() {
                        guard[i] = Some(r);
                    }
                }
            });
        }
    });
    results
        .into_iter()
        .map(|r| {
            r.unwrap_or_else(|| {
                Err(Error::io(
                    "",
                    std::io::Error::other("hashing thread panicked"),
                ))
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_digest() {
        assert_eq!(
            fingerprint_of(b"").as_str(),
            "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            fingerprint_of(b"abc").as_str(),
            "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn reader_and_throttle() {
        let data = vec![7u8; 3 << 20];
        let fast = hash_reader(&data[..], Path::new("mem"), &HashSettings::default()).unwrap();
        assert_eq!(fast.bytes, 3 << 20);
        assert_eq!(fast.fingerprint, fingerprint_of(&data));
        let capped = HashSettings {
            threads: 1,
            bandwidth_mib: 16,
        };
        let slow = hash_reader(&data[..], Path::new("mem"), &capped).unwrap();
        assert_eq!(slow.fingerprint, fast.fingerprint);
        // 3 MiB at 16 MiB/s is at least 187 ms.
        assert!(
            slow.elapsed >= Duration::from_millis(180),
            "{:?}",
            slow.elapsed
        );
    }
}
