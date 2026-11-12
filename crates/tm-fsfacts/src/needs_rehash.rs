//! The re-hash decision (item 30; D19; 3.4; 5.6 `possibly changed
//! (unconfirmed)`): pure, from the previous and current snapshots, the racy
//! result, the settings, and the context.

use crate::snapshot::Snapshot;

/// What a stat comparison decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Same stat facts, not racy: the previous fingerprint stands.
    Unchanged,
    /// Hash the file now.
    HashNow,
    /// Stat changed on a large file during a disk scan: labeled
    /// `possibly changed (unconfirmed)` instead of hashed inline.
    Unconfirmed,
}

/// Where the decision is being made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Context {
    /// `tm status`, a session start, the viewer: stat only above the threshold.
    Scan,
    /// The run wrapper: hashes new or changed entries.
    Run,
    /// `tm audit`: hashes everything.
    Audit,
}

/// The settings the decision reads (11.19).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RehashSettings {
    /// `hashing.inline_threshold`, MiB.
    pub inline_threshold_mib: u64,
    /// `hashing.trust_ctime` (git's `core.trustctime` meaning).
    pub trust_ctime: bool,
}

impl Default for RehashSettings {
    fn default() -> Self {
        RehashSettings {
            inline_threshold_mib: 64,
            trust_ctime: true,
        }
    }
}

/// True when the stat facts differ: `size`, `mtime`, or `ino`, plus `ctime`
/// when `trust_ctime` is set.
#[must_use]
pub fn stat_changed(previous: &Snapshot, current: &Snapshot, trust_ctime: bool) -> bool {
    previous.size != current.size
        || previous.mtime != current.mtime
        || previous.ino != current.ino
        || (trust_ctime && previous.ctime != current.ctime)
}

/// The item 30 decision.
#[must_use]
pub fn decide(
    previous: Option<&Snapshot>,
    current: &Snapshot,
    racy: bool,
    settings: &RehashSettings,
    context: Context,
) -> Decision {
    if context == Context::Audit {
        return Decision::HashNow;
    }
    let changed = match previous {
        None => true,
        Some(p) => stat_changed(p, current, settings.trust_ctime),
    };
    if !changed && !racy {
        return Decision::Unchanged;
    }
    if context == Context::Scan
        && current.size > settings.inline_threshold_mib.saturating_mul(1_048_576)
    {
        return Decision::Unconfirmed;
    }
    Decision::HashNow
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::Time;

    fn snap(size: u64, m: i64, c: i64, ino: u64) -> Snapshot {
        Snapshot {
            size,
            mtime: Time::new(m, 0),
            ctime: Time::new(c, 0),
            ino,
        }
    }

    #[test]
    fn table() {
        let s = RehashSettings::default();
        let a = snap(10, 100, 100, 1);
        assert_eq!(
            decide(Some(&a), &a, false, &s, Context::Scan),
            Decision::Unchanged
        );
        assert_eq!(
            decide(Some(&a), &a, true, &s, Context::Scan),
            Decision::HashNow
        );
        assert_eq!(
            decide(Some(&a), &a, false, &s, Context::Audit),
            Decision::HashNow
        );
        assert_eq!(decide(None, &a, false, &s, Context::Run), Decision::HashNow);
        let ctime_only = snap(10, 100, 101, 1);
        assert_eq!(
            decide(Some(&a), &ctime_only, false, &s, Context::Run),
            Decision::HashNow
        );
        let untrusting = RehashSettings {
            trust_ctime: false,
            ..s
        };
        assert_eq!(
            decide(Some(&a), &ctime_only, false, &untrusting, Context::Run),
            Decision::Unchanged
        );
        let big = snap(65 << 20, 100, 100, 1);
        let big_changed = snap(65 << 20, 101, 101, 1);
        assert_eq!(
            decide(Some(&big), &big_changed, false, &s, Context::Scan),
            Decision::Unconfirmed
        );
        assert_eq!(
            decide(Some(&big), &big_changed, false, &s, Context::Run),
            Decision::HashNow
        );
        assert_eq!(
            decide(None, &big, false, &s, Context::Scan),
            Decision::Unconfirmed
        );
        assert_eq!(
            decide(Some(&a), &snap(10, 100, 100, 2), false, &s, Context::Scan),
            Decision::HashNow
        );
    }
}
