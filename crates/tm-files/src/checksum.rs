//! Provider checksums (item 50; CUJ 1): the GNU `md5sum`/`sha256sum`
//! two-space form and the BSD `MD5 (file) = hash` form, compared once
//! against the fingerprints Files holds, reported as `file.checksum_checked`.

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
/// [`Error::ChecksumLine`] for a line in neither form.
pub fn parse_checksums(text: &str) -> Result<Vec<ChecksumLine>> {
    let mut out = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let line = raw.trim_end_matches('\r');
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let bad = || Error::ChecksumLine {
            line: i + 1,
            text: line.to_owned(),
        };
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

/// Compare parsed lines with the fingerprints Files holds: `entries` pairs
/// each relative name (a single file's own name, or a manifest entry's
/// path) with its `sha256:` fingerprint. Every line must use one
/// algorithm. A listed name that is absent from `entries`, or whose digest
/// differs, is a mismatch; names `entries` has and the file does not list
/// are not checked.
///
/// # Errors
/// [`Error::Unbuilt`] for an MD5 list: computing MD5 needs a dependency
/// that is not on item 1's list (reported as a rule 4 collision);
/// [`Error::ChecksumLine`] for an empty list or mixed algorithms.
pub fn compare(lines: &[ChecksumLine], entries: &[(String, EventId)]) -> Result<Compared> {
    let Some(first) = lines.first() else {
        return Err(Error::ChecksumLine {
            line: 0,
            text: "the checksum file lists nothing".into(),
        });
    };
    let algorithm = first.algorithm;
    if let Some(mixed) = lines.iter().position(|l| l.algorithm != algorithm) {
        return Err(Error::ChecksumLine {
            line: mixed + 1,
            text: "mixed MD5 and SHA-256 lines".into(),
        });
    }
    if algorithm == Algorithm::Md5 {
        return Err(Error::Unbuilt {
            what: "MD5 comparison of provider checksums",
        });
    }
    let mut mismatches = Vec::new();
    for l in lines {
        let name = l.name.strip_prefix("./").unwrap_or(&l.name);
        let found = entries
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, fp)| fp.as_str().trim_start_matches("sha256:") == l.hex);
        if found != Some(true) {
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
        assert!(matches!(
            parse_checksums("not a checksum\n"),
            Err(Error::ChecksumLine { line: 1, .. })
        ));
        assert!(
            matches!(compare(&l2, &[]), Err(Error::ChecksumLine { .. })),
            "mixed"
        );
        assert!(matches!(compare(&l2[1..], &[]), Err(Error::Unbuilt { .. })));
        let fp = EventId::parse(SHA).unwrap();
        let entries = vec![("data/a.txt".to_owned(), fp.clone())];
        let c = compare(&lines, &entries).unwrap();
        assert!(!c.matched);
        assert_eq!(c.mismatches, ["b.bin"]);
        assert_eq!(c.refusal().unwrap().catalog_code(), Some(Code::E3004));
        let c = compare(&lines[..1], &entries).unwrap();
        assert!(c.matched && c.refusal().is_none());
        let ev = c
            .event(FileHandle(3), &VersionHandle::parse("F3@v1").unwrap())
            .unwrap();
        assert_eq!(ev.get("matched"), Some(&serde_json::json!(true)));
    }
}
