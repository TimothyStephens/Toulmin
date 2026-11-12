//! The content-addressed kept-file store (item 36; 11.21 Objects; D4):
//! `objects/sha256/<first two hex>/<remaining hex>.zst`, streamed through
//! SHA-256 and zstd level 3 into `objects/tmp/<random>`, checked against
//! the expected fingerprint, synced, renamed into place.

use std::io::{Read, Write};
use std::path::PathBuf;

use sha2::{Digest, Sha256};
use tm_formats::id::EventId;

use crate::{Code, Error, Record, Result, fsync_dir};

/// zstd level for kept files (11.21).
pub const KEEP_LEVEL: i32 = 3;

/// What `keep` wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kept {
    /// The fingerprint, which is the object's address.
    pub object: EventId,
    /// Raw bytes.
    pub size: u64,
    /// Bytes on disk, for `file.kept.compressed_size`.
    pub compressed_size: u64,
}

impl Record {
    /// `objects/sha256/xx/rest.zst` for a fingerprint.
    #[must_use]
    pub fn object_path(&self, fingerprint: &EventId) -> PathBuf {
        let hex = &fingerprint.as_str()[7..];
        self.objects_dir()
            .join("sha256")
            .join(&hex[..2])
            .join(format!("{}.zst", &hex[2..]))
    }

    /// True when the object is in the store.
    #[must_use]
    pub fn has_object(&self, fingerprint: &EventId) -> bool {
        self.object_path(fingerprint).exists()
    }

    /// Keep a file version's bytes: stream `source`, compare with
    /// `expected`, store under it.
    ///
    /// # Errors
    /// `E3203` when the bytes do not hash to `expected`; `E2008` on a
    /// read-only record; `Error::Io`.
    pub fn keep(&self, source: &mut dyn Read, expected: &EventId) -> Result<Kept> {
        self.writable()?;
        let final_path = self.object_path(expected);
        let tmp_dir = self.objects_dir().join("tmp");
        std::fs::create_dir_all(&tmp_dir).map_err(|e| Error::io(&tmp_dir, e))?;
        let tmp = tmp_dir.join(tm_fsfacts::racy::random_hex());
        let io = |e: std::io::Error| Error::io(&tmp, e);
        let mut hasher = Sha256::new();
        let mut size = 0u64;
        let compressed_size = {
            let file = std::fs::File::create(&tmp).map_err(io)?;
            let mut enc = zstd::stream::write::Encoder::new(file, KEEP_LEVEL).map_err(io)?;
            enc.include_checksum(true).map_err(io)?;
            let mut buf = vec![0u8; 1 << 20];
            loop {
                let n = source.read(&mut buf).map_err(|e| {
                    let _ = std::fs::remove_file(&tmp);
                    Error::io("<source>", e)
                })?;
                if n == 0 {
                    break;
                }
                hasher.update(&buf[..n]);
                size += n as u64;
                enc.write_all(&buf[..n]).map_err(io)?;
            }
            let file = enc.finish().map_err(io)?;
            file.sync_all().map_err(io)?;
            file.metadata().map_err(io)?.len()
        };
        let computed = crate::segment::id_from_digest(&hasher.finalize());
        if computed != *expected {
            let _ = std::fs::remove_file(&tmp);
            return Err(Error::code(
                Code::E3203,
                format!("bytes hash to {computed}, not the expected {expected}"),
            ));
        }
        if final_path.exists() {
            // Already kept: the store is content-addressed, so the existing
            // object is this one.
            let _ = std::fs::remove_file(&tmp);
            let existing = std::fs::metadata(&final_path)
                .map_err(|e| Error::io(&final_path, e))?
                .len();
            return Ok(Kept {
                object: expected.clone(),
                size,
                compressed_size: existing,
            });
        }
        let dir = final_path.parent().unwrap_or(&final_path).to_path_buf();
        std::fs::create_dir_all(&dir).map_err(|e| Error::io(&dir, e))?;
        std::fs::rename(&tmp, &final_path).map_err(|e| Error::io(&final_path, e))?;
        fsync_dir(&dir)?;
        Ok(Kept {
            object: expected.clone(),
            size,
            compressed_size,
        })
    }

    /// Read a kept object's bytes back, decompressed.
    ///
    /// # Errors
    /// `E3203` when the object is absent; `Error::Io`.
    pub fn open_object(&self, fingerprint: &EventId) -> Result<Box<dyn Read>> {
        let path = self.object_path(fingerprint);
        let file = std::fs::File::open(&path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Error::code(Code::E3203, format!("object {fingerprint} is not kept"))
            } else {
                Error::io(&path, e)
            }
        })?;
        let dec = zstd::stream::read::Decoder::new(file).map_err(|e| Error::io(&path, e))?;
        Ok(Box::new(dec))
    }

    /// Every object in the store, by fingerprint, in path order.
    ///
    /// # Errors
    /// `Error::Io`.
    pub fn objects(&self) -> Result<Vec<EventId>> {
        let base = self.objects_dir().join("sha256");
        let mut out = Vec::new();
        // Git does not keep empty directories, so a record checked out as a
        // fixture may lack `objects/sha256/`: that is an empty store.
        let entries = match std::fs::read_dir(&base) {
            Ok(rd) => rd,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
            Err(e) => return Err(Error::io(&base, e)),
        };
        let mut dirs: Vec<PathBuf> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_dir())
            .collect();
        dirs.sort();
        for d in dirs {
            let prefix = d
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_owned();
            let mut files: Vec<PathBuf> = std::fs::read_dir(&d)
                .map_err(|e| Error::io(&d, e))?
                .filter_map(|e| e.ok().map(|e| e.path()))
                .collect();
            files.sort();
            for f in files {
                let name = f.file_name().and_then(|n| n.to_str()).unwrap_or_default();
                if let Some(rest) = name.strip_suffix(".zst")
                    && let Some(id) = EventId::parse(&format!("sha256:{prefix}{rest}"))
                {
                    out.push(id);
                }
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keep_checks_the_fingerprint_and_reads_back() {
        let dir = tempfile::tempdir().unwrap();
        let r = Record::create(dir.path(), "p", "o", "2026-11-11T12:00:00Z").unwrap();
        let bytes = b"alpha,beta\n1,2\n";
        let fp = tm_fsfacts::hash::fingerprint_of(bytes);
        let kept = r.keep(&mut &bytes[..], &fp).unwrap();
        assert_eq!(kept.object, fp);
        assert_eq!(kept.size, bytes.len() as u64);
        assert!(kept.compressed_size > 0);
        assert!(r.has_object(&fp));
        assert_eq!(
            r.object_path(&fp)
                .strip_prefix(dir.path())
                .unwrap()
                .to_str()
                .unwrap(),
            format!(
                "objects/sha256/{}/{}.zst",
                &fp.as_str()[7..9],
                &fp.as_str()[9..]
            )
        );
        let mut back = Vec::new();
        r.open_object(&fp).unwrap().read_to_end(&mut back).unwrap();
        assert_eq!(back, bytes);
        // Kept twice: the same object.
        let again = r.keep(&mut &bytes[..], &fp).unwrap();
        assert_eq!(again.compressed_size, kept.compressed_size);
        assert_eq!(r.objects().unwrap(), vec![fp.clone()]);
        // Wrong bytes: E3203 and nothing stored.
        let other = tm_fsfacts::hash::fingerprint_of(b"other");
        let err = r.keep(&mut &bytes[..], &other).unwrap_err();
        assert_eq!(err.catalog_code(), Some(Code::E3203));
        assert!(!r.has_object(&other));
        assert_eq!(
            std::fs::read_dir(r.objects_dir().join("tmp"))
                .unwrap()
                .count(),
            0,
            "no temp file left"
        );
        match r.open_object(&other) {
            Err(e) => assert_eq!(e.catalog_code(), Some(Code::E3203)),
            Ok(_) => panic!("an absent object opened"),
        }
    }
}
