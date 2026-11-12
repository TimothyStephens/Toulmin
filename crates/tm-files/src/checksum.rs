//! Provider checksums (item 50; CUJ 1): the GNU `md5sum`/`sha256sum`
//! two-space form and the BSD `MD5 (file) = hash` form, compared once
//! against the fingerprints Files holds, reported as `file.checksum_checked`.

use std::fmt::Write as _;
use std::io::Read;
use std::path::{Path, PathBuf};

use md5::{Digest, Md5};
use serde_json::json;
use tm_formats::events::{EventType, KnownEvent};
use tm_formats::id::{EventId, FileHandle, VersionHandle};

use crate::{Code, Error, Result, event};

/// The digest a checksum file carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Algorithm {
    /// 32 hex digits.
    Md5,
    /// 64 hex digits.
    Sha256,
}

impl Algorithm {
    /// The 11.16 spelling.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Algorithm::Md5 => "md5",
            Algorithm::Sha256 => "sha256",
        }
    }

    fn by_hex_len(hex: &str) -> Option<Algorithm> {
        if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        match hex.len() {
            32 => Some(Algorithm::Md5),
            64 => Some(Algorithm::Sha256),
            _ => None,
        }
    }
}

/// One line of a checksum file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChecksumLine {
    /// Which digest.
    pub algorithm: Algorithm,
    /// Lower-case hex.
    pub hex: String,
    /// The name as written (relative to the checksum file's directory).
    pub name: String,
}

/// Parse a checksum file. Blank lines and `#` comments are skipped; every
/// other line is the GNU form (`<hex>  <name>` or `<hex> *<name>`) or the
/// BSD form (`MD5 (<name>) = <hex>`, `SHA256 (<name>) = <hex>`).
///
/// # Errors
/// `E3006` for a line in neither form.
pub fn parse_checksums(text: &str) -> Result<Vec<ChecksumLine>> {
    let mut out = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let line = raw.trim_end_matches('\r');
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let bad = || unreadable(i + 1, line);
        // BSD: ALG (name) = hex
        if let Some((alg, rest)) = line.split_once(" (")
            && let Some((name, hex)) = rest.rsplit_once(") = ")
        {
            let algorithm = match alg {
                "MD5" => Algorithm::Md5,
                "SHA256" => Algorithm::Sha256,
                _ => return Err(bad()),
            };
            let hex = hex.trim();
            if Algorithm::by_hex_len(hex) != Some(algorithm) {
                return Err(bad());
            }
            out.push(ChecksumLine {
                algorithm,
                hex: hex.to_ascii_lowercase(),
                name: name.to_owned(),
            });
            continue;
        }
        // GNU: hex, two spaces (text) or space-star (binary), name.
        let Some((hex, name)) = line.split_once("  ").or_else(|| line.split_once(" *")) else {
            return Err(bad());
        };
        let Some(algorithm) = Algorithm::by_hex_len(hex) else {
            return Err(bad());
        };
        if name.is_empty() {
            return Err(bad());
        }
        out.push(ChecksumLine {
            algorithm,
            hex: hex.to_ascii_lowercase(),
            name: name.to_owned(),
        });
    }
    Ok(out)
}

fn unreadable(line: usize, text: &str) -> Error {
    Error::code(
        Code::E3006,
        format!("checksum list line {line} is not a checksum line: {text:?}"),
    )
}

/// One file Files holds a fingerprint for, as the comparison sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The relative name a list would use (a file's own name, or a manifest
    /// entry's path).
    pub name: String,
    /// Where the bytes are, for an MD5 list.
    pub path: PathBuf,
    /// The `sha256:` fingerprint.
    pub fingerprint: EventId,
}

/// The MD5 of a file's bytes as lower-case hex.
///
/// # Errors
/// `Error::Io`.
pub fn md5_hex(path: &Path) -> Result<String> {
    let mut f = std::fs::File::open(path).map_err(|e| Error::io(path, e))?;
    let mut h = Md5::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = f.read(&mut buf).map_err(|e| Error::io(path, e))?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    let mut hex = String::with_capacity(32);
    for b in h.finalize() {
        let _ = write!(hex, "{b:02x}");
    }
    Ok(hex)
}

/// The one-time comparison's result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Compared {
    /// The algorithm the file used.
    pub algorithm: Algorithm,
    /// True when every listed name matched.
    pub matched: bool,
    /// Listed names whose digest differed or that Files has no entry for.
    pub mismatches: Vec<String>,
}

impl Compared {
    /// The `file.checksum_checked` event (11.16).
    ///
    /// # Errors
    /// `E1002` if the body does not validate (it always does).
    pub fn event(&self, file: FileHandle, version: &VersionHandle) -> Result<KnownEvent> {
        event(
            EventType::FileChecksumChecked,
            &json!({
                "file": file.to_string(),
                "version": version.to_string(),
                "algorithm": self.algorithm.as_str(),
                "matched": self.matched,
                "mismatches": self.mismatches,
            }),
        )
    }

    /// The refusal `file.register` makes when `matched` is false (section
    /// 9's `E3004`).
    #[must_use]
    pub fn refusal(&self) -> Option<Error> {
        (!self.matched).then(|| {
            Error::code(
                Code::E3004,
                format!(
                    "{} checksum mismatch for {}",
                    self.algorithm.as_str(),
                    self.mismatches.join(", ")
                ),
            )
        })
    }
}

/// Compare parsed lines with what Files holds: for a SHA-256 list the
/// entries' fingerprints, for an MD5 list the MD5 of each listed entry's
/// bytes (item 50: MD5 is used only here; every Toulmin fingerprint is
/// sha256). Every line must use one algorithm. A listed name that is absent
/// from `entries`, or whose digest differs, is a mismatch; names `entries`
/// has and the list does not name are not checked.
///
/// # Errors
/// `E3006` for an empty list or mixed algorithms; `Error::Io` when an
/// MD5 list names a file that cannot be read.
pub fn compare(lines: &[ChecksumLine], entries: &[Entry]) -> Result<Compared> {
    let Some(first) = lines.first() else {
        return Err(unreadable(0, "the checksum list names nothing"));
    };
    let algorithm = first.algorithm;
    if let Some(mixed) = lines.iter().position(|l| l.algorithm != algorithm) {
        return Err(unreadable(mixed + 1, "mixed MD5 and SHA-256 lines"));
    }
    let mut mismatches = Vec::new();
    for l in lines {
        let name = l.name.strip_prefix("./").unwrap_or(&l.name);
        let entry = entries.iter().find(|e| e.name == name);
        let matched = match (entry, algorithm) {
            (None, _) => false,
            (Some(e), Algorithm::Sha256) => {
                e.fingerprint.as_str().trim_start_matches("sha256:") == l.hex
            }
            (Some(e), Algorithm::Md5) => md5_hex(&e.path)? == l.hex,
        };
        if !matched {
            mismatches.push(l.name.clone());
        }
    }
    Ok(Compared {
        algorithm,
        matched: mismatches.is_empty(),
        mismatches,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHA: &str = "sha256:9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";

    #[test]
    fn both_forms_parse_and_compare() {
        let gnu = format!(
            "{}  data/a.txt\n{} *b.bin\n\n# comment\n",
            &SHA[7..],
            &SHA[7..]
        );
        let lines = parse_checksums(&gnu).unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].algorithm, Algorithm::Sha256);
        assert_eq!(lines[1].name, "b.bin");
        let bsd = format!(
            "SHA256 (data/a.txt) = {}\nMD5 (x) = d41d8cd98f00b204e9800998ecf8427e\n",
            &SHA[7..]
        );
        let l2 = parse_checksums(&bsd).unwrap();
        assert_eq!(l2[0].name, "data/a.txt");
        assert_eq!(l2[1].algorithm, Algorithm::Md5);
        assert_eq!(
            parse_checksums("not a checksum\n")
                .unwrap_err()
                .catalog_code(),
            Some(Code::E3006)
        );
        assert_eq!(
            compare(&l2, &[]).unwrap_err().catalog_code(),
            Some(Code::E3006),
            "mixed"
        );
        let fp = EventId::parse(SHA).unwrap();
        let entries = vec![Entry {
            name: "data/a.txt".to_owned(),
            path: PathBuf::new(),
            fingerprint: fp.clone(),
        }];
        let c = compare(&lines, &entries).unwrap();
        assert!(!c.matched);
        assert_eq!(c.mismatches, ["b.bin"]);
        assert_eq!(c.refusal().unwrap().catalog_code(), Some(Code::E3004));
        let c = compare(&lines[..1], &entries).unwrap();
        assert!(c.matched && c.refusal().is_none());
        // An MD5 list hashes the entries' bytes.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), b"").unwrap();
        let md5 = parse_checksums("d41d8cd98f00b204e9800998ecf8427e  a.txt\n").unwrap();
        let e = vec![Entry {
            name: "a.txt".into(),
            path: dir.path().join("a.txt"),
            fingerprint: fp,
        }];
        assert!(compare(&md5, &e).unwrap().matched, "md5 of the empty file");
        let ev = c
            .event(FileHandle(3), &VersionHandle::parse("F3@v1").unwrap())
            .unwrap();
        assert_eq!(ev.get("matched"), Some(&serde_json::json!(true)));
    }
}
