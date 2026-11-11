//! Format version constants and the reader policy (design 11.11, item 21).
//!
//! Every format carries its own version. Readers accept any version they
//! know; an unknown event type or a version above the highest known is read
//! as opaque and never rewritten; writing an unknown version is refused.

/// `record.json` `format` and `HEAD` `format`.
pub const RECORD_FORMAT: u64 = 1;
/// Envelope `env`.
pub const ENVELOPE: u64 = 1;
/// Receipt `receipt` (gate C).
pub const RECEIPT: u64 = 1;
/// Parse spec `spec`.
pub const PARSE_SPEC: u64 = 1;
/// Query steps `steps`.
pub const STEPS: u64 = 1;
/// Assertion `assert`.
pub const ASSERTION: u64 = 1;
/// Handshake `handshake` (gate B).
pub const HANDSHAKE: u64 = 1;
/// Action file `actions`.
pub const ACTIONS: u64 = 1;

/// How a reader treats a version it sees.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Read {
    /// A known version: read and interpret.
    Known,
    /// Above every known version: keep as given, never interpret or rewrite.
    Opaque,
    /// Neither known nor above the known ones (for example 0): invalid.
    Invalid,
}

/// The reader policy for one format whose known versions are `known`.
#[must_use]
pub fn read_policy(known: &[u64], seen: u64) -> Read {
    if known.contains(&seen) {
        Read::Known
    } else if known.iter().all(|k| seen > *k) {
        Read::Opaque
    } else {
        Read::Invalid
    }
}

/// The writer policy: only known versions may be written.
#[must_use]
pub fn may_write(known: &[u64], version: u64) -> bool {
    known.contains(&version)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy() {
        assert_eq!(read_policy(&[1], 1), Read::Known);
        assert_eq!(read_policy(&[1], 2), Read::Opaque);
        assert_eq!(read_policy(&[1], 0), Read::Invalid);
        assert!(may_write(&[1], 1));
        assert!(!may_write(&[1], 2));
    }
}
