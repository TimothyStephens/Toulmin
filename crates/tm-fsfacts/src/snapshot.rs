//! Stat snapshots (item 26; 11.16's `snapshot` object): size, mtime, ctime,
//! and inode, no device number, taken with `statx` and `STATX_FORCE_SYNC`
//! where the filesystem supports it, else by opening the file first so a
//! network client revalidates its attributes.

use std::fs::File;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use rustix::fs::{AtFlags, CWD, StatxFlags, statx};
use serde_json::{Map, Value, json};

use crate::Error;
use crate::time::Time;

/// One file's stat facts (D6, D19).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Snapshot {
    /// Size in bytes.
    pub size: u64,
    /// Last modification time.
    pub mtime: Time,
    /// Last inode change time.
    pub ctime: Time,
    /// Inode number (unsigned 64-bit; written as a decimal string, 11.16).
    pub ino: u64,
}

impl Snapshot {
    /// Snapshot a path, following a final symbolic link.
    ///
    /// # Errors
    /// `Error::Io` when the path cannot be stat'ed.
    pub fn take(path: &Path) -> Result<Snapshot, Error> {
        match statx(
            CWD,
            path,
            AtFlags::STATX_FORCE_SYNC,
            StatxFlags::SIZE | StatxFlags::MTIME | StatxFlags::CTIME | StatxFlags::INO,
        ) {
            Ok(st) => Ok(Snapshot {
                size: st.stx_size,
                mtime: Time::new(st.stx_mtime.tv_sec, st.stx_mtime.tv_nsec),
                ctime: Time::new(st.stx_ctime.tv_sec, st.stx_ctime.tv_nsec),
                ino: st.stx_ino,
            }),
            // No statx, or the flag is refused here: open first, then stat
            // through the descriptor, which makes a network client fetch
            // fresh attributes.
            Err(rustix::io::Errno::NOSYS | rustix::io::Errno::INVAL) => {
                let file = File::open(path).map_err(|e| Error::io(path, e))?;
                let md = file.metadata().map_err(|e| Error::io(path, e))?;
                Ok(Snapshot::from_metadata(&md))
            }
            Err(e) => Err(Error::io(path, e.into())),
        }
    }

    /// From `std` metadata (used by the fallback and by callers that already
    /// hold one).
    #[must_use]
    pub fn from_metadata(md: &std::fs::Metadata) -> Snapshot {
        Snapshot {
            size: md.size(),
            mtime: Time::new(md.mtime(), u32::try_from(md.mtime_nsec()).unwrap_or(0)),
            ctime: Time::new(md.ctime(), u32::try_from(md.ctime_nsec()).unwrap_or(0)),
            ino: md.ino(),
        }
    }

    /// The 11.16 `snapshot` object.
    #[must_use]
    pub fn to_json(self) -> Value {
        json!({
            "size": self.size,
            "mtime": self.mtime.rfc3339_seconds(),
            "mtime_ns": self.mtime.nanos,
            "ctime": self.ctime.rfc3339_seconds(),
            "ctime_ns": self.ctime.nanos,
            "ino": self.ino.to_string(),
        })
    }

    /// Read the 11.16 `snapshot` object; `None` when it is not one.
    #[must_use]
    pub fn from_json(v: &Value) -> Option<Snapshot> {
        let m: &Map<String, Value> = v.as_object()?;
        if m.len() != 6 {
            return None;
        }
        let int = |k: &str| m.get(k)?.as_u64();
        let time = |k: &str, ns: &str| {
            let t = Time::parse_rfc3339_seconds(m.get(k)?.as_str()?)?;
            let nanos = u32::try_from(int(ns)?).ok()?;
            (nanos < 1_000_000_000).then_some(Time::new(t.secs, nanos))
        };
        let ino = m.get("ino")?.as_str()?;
        if ino != "0" && (ino.starts_with('0') || !ino.bytes().all(|b| b.is_ascii_digit())) {
            return None;
        }
        Some(Snapshot {
            size: int("size")?,
            mtime: time("mtime", "mtime_ns")?,
            ctime: time("ctime", "ctime_ns")?,
            ino: ino.parse().ok()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_round_trip() {
        let s = Snapshot {
            size: 1_048_576,
            mtime: Time::new(1_762_696_931, 123_456_789),
            ctime: Time::new(1_762_696_931, 123_456_789),
            ino: 18_446_744_073_709_551_615,
        };
        let v = s.to_json();
        assert_eq!(v["ino"], "18446744073709551615");
        assert_eq!(v["mtime"], "2025-11-09T14:02:11Z");
        assert_eq!(v["mtime_ns"], 123_456_789);
        assert_eq!(Snapshot::from_json(&v), Some(s));
        let mut bad = v.clone();
        bad["ino"] = json!(4211);
        assert_eq!(Snapshot::from_json(&bad), None);
        bad["ino"] = json!("0421");
        assert_eq!(Snapshot::from_json(&bad), None);
    }

    #[test]
    fn takes_a_real_file() {
        let s = Snapshot::take(Path::new("Cargo.toml")).unwrap();
        assert!(s.size > 0);
        assert!(s.ino > 0);
        let md = std::fs::metadata("Cargo.toml").unwrap();
        assert_eq!(s, Snapshot::from_metadata(&md));
        assert!(Snapshot::take(Path::new("no-such-file")).is_err());
    }
}
