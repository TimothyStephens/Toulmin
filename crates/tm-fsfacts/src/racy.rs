//! The racy rule (item 29; D19; Risks): a file whose mtime is at or after
//! the marker's is always re-hashed, because its content may have changed
//! within the clock's granularity after the snapshot was taken. The marker
//! and the file are timed by the same filesystem, so clock skew between
//! nodes does not enter.

use std::fmt::Write as _;
use std::fs::OpenOptions;
use std::path::Path;

use crate::Error;
use crate::snapshot::Snapshot;
use crate::time::Time;

/// The marker file's mtime, taken from the filesystem the caller names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Marker {
    /// The marker's modification time as the filesystem reported it.
    pub mtime: Time,
}

impl Marker {
    /// Create and remove `.toulmin-marker-<random>` in `dir`, returning its
    /// mtime. `dir` must be on the filesystem of the files to be compared
    /// (the repository root for a disk scan, the output's directory for the
    /// wrapper).
    ///
    /// # Errors
    /// `Error::Io` when `dir` is not writable.
    pub fn touch(dir: &Path) -> Result<Marker, Error> {
        let name = format!(".toulmin-marker-{}", random_hex());
        let path = dir.join(name);
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| Error::io(&path, e))?;
        let snapshot = Snapshot::take(&path);
        let _ = std::fs::remove_file(&path);
        Ok(Marker {
            mtime: snapshot?.mtime,
        })
    }
}

/// True when `snapshot` is racy against `marker`: its mtime is at or after
/// the marker's.
#[must_use]
pub fn is_racy(snapshot: &Snapshot, marker: &Marker) -> bool {
    snapshot.mtime >= marker.mtime
}

/// Sixteen hex digits from the kernel's random source.
#[must_use]
pub fn random_hex() -> String {
    let mut buf = [0u8; 8];
    let mut filled = 0;
    while filled < buf.len() {
        match rustix::rand::getrandom(&mut buf[filled..], rustix::rand::GetRandomFlags::empty()) {
            Ok(n) => filled += n,
            Err(rustix::io::Errno::INTR) => {}
            // Before the random pool is ready, or on a kernel without
            // getrandom: fall back to time and address entropy.
            Err(_) => {
                let t = Time::now();
                let seed = t.secs.cast_unsigned()
                    ^ (u64::from(t.nanos) << 32)
                    ^ ((&raw const buf) as usize as u64);
                buf = seed.to_le_bytes();
                break;
            }
        }
    }
    buf.iter().fold(String::with_capacity(16), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marker_and_rule() {
        let dir = std::env::temp_dir();
        let m = Marker::touch(&dir).unwrap();
        let s = Snapshot {
            size: 1,
            mtime: m.mtime,
            ctime: m.mtime,
            ino: 1,
        };
        assert!(is_racy(&s, &m));
        let earlier = Snapshot {
            mtime: Time::new(m.mtime.secs - 1, m.mtime.nanos),
            ..s
        };
        assert!(!is_racy(&earlier, &m));
        assert_eq!(random_hex().len(), 16);
        assert_ne!(random_hex(), random_hex());
    }
}
