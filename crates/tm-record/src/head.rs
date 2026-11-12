//! `main/HEAD` (item 34; 11.3; 11.21): read with the directory refreshed,
//! written by write-then-rename, and the lock-time rewrite.

use std::path::Path;

use serde_json::{Value, json};
use tm_formats::id::EventId;

use crate::lock::read_lock_file;
use crate::{Code, Error, Result, fsync_dir, read_small_json, refresh_dir, write_small_json};

/// `{"seq","segment","offset","last_id","batch","epoch","format":1}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Head {
    /// The last event's `seq` (0 before any event).
    pub seq: u64,
    /// The open segment's number.
    pub segment: u64,
    /// The open segment's committed length in bytes.
    pub offset: u64,
    /// The last event's ID (the genesis string before any event).
    pub last_id: EventId,
    /// The last batch's number (0 before any batch).
    pub batch: u64,
    /// The epoch of the last lock holder.
    pub epoch: u64,
    /// The `HEAD` format, 1.
    pub format: u64,
}

impl Head {
    /// `project.init`'s `HEAD` (11.3).
    #[must_use]
    pub fn initial(genesis: EventId) -> Head {
        Head {
            seq: 0,
            segment: 1,
            offset: 0,
            last_id: genesis,
            batch: 0,
            epoch: 0,
            format: 1,
        }
    }

    /// The JSON object.
    #[must_use]
    pub fn to_json(&self) -> Value {
        json!({
            "seq": self.seq,
            "segment": self.segment,
            "offset": self.offset,
            "last_id": self.last_id.as_str(),
            "batch": self.batch,
            "epoch": self.epoch,
            "format": self.format,
        })
    }

    /// Parse the JSON object; `None` when it is not a `HEAD`.
    #[must_use]
    pub fn from_json(v: &Value) -> Option<Head> {
        let m = v.as_object()?;
        if m.len() != 7 {
            return None;
        }
        let int = |k: &str| m.get(k)?.as_u64();
        Some(Head {
            seq: int("seq")?,
            segment: int("segment")?,
            offset: int("offset")?,
            last_id: EventId::parse(m.get("last_id")?.as_str()?)?,
            batch: int("batch")?,
            epoch: int("epoch")?,
            format: int("format")?,
        })
    }

    /// Read `main/HEAD` after refreshing the directory (3.1).
    ///
    /// # Errors
    /// `E2103` when `HEAD` is missing or malformed; `Error::Io`.
    pub fn read(main: &Path) -> Result<Head> {
        refresh_dir(main);
        let path = main.join("HEAD");
        let v = read_small_json(&path)?;
        let head = Head::from_json(&v)
            .ok_or_else(|| Error::code(Code::E2103, "main/HEAD is not a HEAD file"))?;
        if head.segment == 0 || head.format != 1 {
            return Err(Error::code(
                Code::E2103,
                "main/HEAD has segment 0 or an unknown format",
            ));
        }
        Ok(head)
    }

    /// Write `HEAD` by write-then-rename (11.21 step 3): `HEAD.tmp`,
    /// `fsync`, rename, `fsync` the directory.
    ///
    /// # Errors
    /// `Error::Io`.
    pub fn write(&self, main: &Path) -> Result<()> {
        self.write_checked(main, None)
    }

    /// [`Head::write`] with the lock re-read of 11.21 step 3: when
    /// `expected_epoch` is given, `main/lock` must exist and carry it, or
    /// the write stops with `E2102`.
    ///
    /// # Errors
    /// `E2102` when the lock has changed hands; `Error::Io`.
    pub fn write_checked(&self, main: &Path, expected_epoch: Option<u64>) -> Result<()> {
        let tmp = main.join("HEAD.tmp");
        let path = main.join("HEAD");
        write_small_json(&tmp, &self.to_json())?;
        if let Some(epoch) = expected_epoch {
            match read_lock_file(main)? {
                Some(holder) if holder.epoch == epoch => {}
                Some(holder) => {
                    let _ = std::fs::remove_file(&tmp);
                    return Err(Error::code(
                        Code::E2102,
                        format!(
                            "the lock was taken over during the write: it now holds epoch {}, this writer has {epoch}",
                            holder.epoch
                        ),
                    ));
                }
                None => {
                    let _ = std::fs::remove_file(&tmp);
                    return Err(Error::code(
                        Code::E2102,
                        "the lock file disappeared during the write",
                    ));
                }
            }
        }
        std::fs::rename(&tmp, &path).map_err(|e| Error::io(&path, e))?;
        fsync_dir(main)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tm_formats::id::genesis;

    #[test]
    fn json_and_files() {
        let g = genesis("main", "p");
        let h = Head::initial(g.clone());
        let v = h.to_json();
        assert_eq!(v["segment"], 1);
        assert_eq!(v["last_id"], g.as_str());
        assert_eq!(Head::from_json(&v), Some(h.clone()));
        let dir = tempfile::tempdir().unwrap();
        h.write(dir.path()).unwrap();
        assert_eq!(Head::read(dir.path()).unwrap(), h);
        let text = std::fs::read_to_string(dir.path().join("HEAD")).unwrap();
        assert!(text.starts_with("{\"batch\":0,\"epoch\":0,\"format\":1,\"last_id\":\"sha256:"));
        assert!(text.ends_with("}\n"));
        assert!(!dir.path().join("HEAD.tmp").exists());
        // The checked write needs the lock file.
        let err = h.write_checked(dir.path(), Some(3)).unwrap_err();
        assert_eq!(err.catalog_code(), Some(Code::E2102));
    }
}
