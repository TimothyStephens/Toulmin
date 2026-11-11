//! IDs and handles (design 11.2 and 11.16, item 12).

use std::fmt;

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::canonical;
use crate::errors::Error;

/// `sha256:` followed by 64 lower-case hex digits: an event ID, a
/// fingerprint, or any other content address.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EventId(String);

/// The prefix of every ID.
pub const PREFIX: &str = "sha256:";

impl EventId {
    /// Parse the text form.
    #[must_use]
    pub fn parse(s: &str) -> Option<EventId> {
        is_sha256(s).then(|| EventId(s.to_owned()))
    }

    /// The text form.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for EventId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// True when `s` is `sha256:` followed by 64 lower-case hex digits.
#[must_use]
pub fn is_sha256(s: &str) -> bool {
    s.strip_prefix(PREFIX)
        .is_some_and(|h| h.len() == 64 && h.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')))
}

/// `"sha256:" + hex(sha256(bytes))` over already-canonical bytes.
#[must_use]
pub fn derive_id(canonical_bytes: &[u8]) -> EventId {
    let digest = Sha256::digest(canonical_bytes);
    let mut s = String::with_capacity(71);
    s.push_str(PREFIX);
    for b in digest {
        s.push(char::from(b"0123456789abcdef"[usize::from(b >> 4)]));
        s.push(char::from(b"0123456789abcdef"[usize::from(b & 15)]));
    }
    EventId(s)
}

/// The ID of a JSON value: the hash of its canonical bytes.
///
/// # Errors
/// `E1001` if the value cannot be canonicalized.
pub fn id_of(v: &Value) -> Result<EventId, Error> {
    Ok(derive_id(&canonical::canonical_bytes(v)?))
}

/// The genesis string of a chain: the `prev` of its first event (11.2).
#[must_use]
pub fn genesis(chain: &str, project_id: &str) -> EventId {
    // Two strings never fail to canonicalize.
    let v = json!({"chain": chain, "project_id": project_id});
    derive_id(canonical::canonical(&v).unwrap_or_default().as_bytes())
}

/// Parse `<prefix><n>` with `n` a decimal integer from 1 without leading
/// zeros.
fn number_after(s: &str, prefix: char) -> Option<u64> {
    let rest = s.strip_prefix(prefix)?;
    positive(rest)
}

/// A decimal integer from 1 without leading zeros.
#[must_use]
pub fn positive(s: &str) -> Option<u64> {
    if s.is_empty() || s.starts_with('0') || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

/// `F<n>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FileHandle(pub u64);

impl FileHandle {
    /// Parse `F<n>`.
    #[must_use]
    pub fn parse(s: &str) -> Option<FileHandle> {
        number_after(s, 'F').map(FileHandle)
    }
}

impl fmt::Display for FileHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "F{}", self.0)
    }
}

/// `F<n>@v<m>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VersionHandle {
    /// The file.
    pub file: FileHandle,
    /// The version number, from 1.
    pub version: u64,
}

impl VersionHandle {
    /// Parse `F<n>@v<m>`.
    #[must_use]
    pub fn parse(s: &str) -> Option<VersionHandle> {
        let (f, v) = s.split_once("@v")?;
        Some(VersionHandle {
            file: FileHandle::parse(f)?,
            version: positive(v)?,
        })
    }
}

impl fmt::Display for VersionHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@v{}", self.file, self.version)
    }
}

/// A claim ref: `C<n>` (owner) or `S<n>.<m>` (session). The form is the
/// same in every gate (11.16); gate A operations allocate only `C<n>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ClaimRef {
    /// `C<n>`.
    Owner(u64),
    /// `S<n>.<m>`: claim `m` of session `n`.
    Session(u64, u64),
}

impl ClaimRef {
    /// Parse `C<n>` or `S<n>.<m>`.
    #[must_use]
    pub fn parse(s: &str) -> Option<ClaimRef> {
        if let Some(n) = number_after(s, 'C') {
            return Some(ClaimRef::Owner(n));
        }
        let (session, m) = s.strip_prefix('S')?.split_once('.')?;
        Some(ClaimRef::Session(positive(session)?, positive(m)?))
    }
}

impl fmt::Display for ClaimRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ClaimRef::Owner(n) => write!(f, "C{n}"),
            ClaimRef::Session(n, m) => write!(f, "S{n}.{m}"),
        }
    }
}

/// `T<n>`, a report handle (11.16), allocated by `report.add`. `R<n>` is
/// a run handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ReportHandle(pub u64);

impl ReportHandle {
    /// Parse `T<n>`.
    #[must_use]
    pub fn parse(s: &str) -> Option<ReportHandle> {
        number_after(s, 'T').map(ReportHandle)
    }
}

impl fmt::Display for ReportHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "T{}", self.0)
    }
}

/// True for a run handle `R<n>` (11.16's `origin.run`).
#[must_use]
pub fn is_run_handle(s: &str) -> bool {
    number_after(s, 'R').is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles() {
        assert_eq!(VersionHandle::parse("F7@v2").unwrap().to_string(), "F7@v2");
        assert!(VersionHandle::parse("F07@v2").is_none());
        assert!(VersionHandle::parse("F7@v0").is_none());
        assert!(FileHandle::parse("F").is_none());
        assert_eq!(ClaimRef::parse("C31"), Some(ClaimRef::Owner(31)));
        assert_eq!(ClaimRef::parse("S4.12"), Some(ClaimRef::Session(4, 12)));
        assert_eq!(ClaimRef::parse("S4.12").unwrap().to_string(), "S4.12");
        for bad in ["C07", "S4", "S4.", "S04.1", "S4.0", "S4.1.2", "c3"] {
            assert!(ClaimRef::parse(bad).is_none(), "{bad}");
        }
        assert!(is_run_handle("R12"));
        assert_eq!(ReportHandle::parse("T3").unwrap().to_string(), "T3");
        assert!(ReportHandle::parse("paper.md").is_none());
        assert!(ReportHandle::parse("R3").is_none());
        assert!(ReportHandle::parse("T0").is_none());
    }

    #[test]
    fn known_digest() {
        // sha256 of the empty string.
        assert_eq!(
            derive_id(b"").as_str(),
            "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
