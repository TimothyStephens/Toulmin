//! The 7.3 evidence-exists outcome table as an exhaustive match (item 65;
//! G1), with the column reading, the combination into one `evidence`
//! value, and the `evidence.detail` text 7.3 states.
//!
//! The Claims library does not read bytes. The Core classifies the cited
//! version ([`VersionFacts`] to [`Situation`]) and, when a cell says the
//! check runs against the pinned version ([`Base::Check`]), answers the
//! citation type's test from Files: every key present (cells), page within
//! the page count (page), label registered (panel).

use serde_json::{Value, json};

/// A citation type: a row of 7.3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CitationType {
    /// `cells` by key.
    Cells,
    /// `whole_file`.
    WholeFile,
    /// `page`.
    Page,
    /// `panel`.
    Panel,
    /// `quote`, `page_region`, `image_region` (11.13).
    Reserved,
}

impl CitationType {
    /// The row a citation falls in.
    #[must_use]
    pub fn of(c: &tm_formats::citation::Citation) -> CitationType {
        use tm_formats::citation::Citation;
        match c {
            Citation::Cells { .. } => CitationType::Cells,
            Citation::WholeFile { .. } => CitationType::WholeFile,
            Citation::Page { .. } => CitationType::Page,
            Citation::Panel { .. } => CitationType::Panel,
            Citation::Reserved(_) => CitationType::Reserved,
        }
    }

    fn fails_reason(self) -> &'static str {
        match self {
            CitationType::Cells => "a cited key is absent",
            CitationType::Page => "page beyond the page count",
            CitationType::Panel => "panel not registered",
            CitationType::WholeFile | CitationType::Reserved => "",
        }
    }
}

/// The cited version's situation at check time: a column of 7.3, the
/// `Retired` column being the `retired` argument of [`cell`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Situation {
    /// Kept, or on disk and matching the pinned fingerprint.
    KeptOrMatches,
    /// On disk, bytes differ from the pinned fingerprint; `kept` when the
    /// pinned bytes are kept.
    DiskDiffers {
        /// The pinned bytes are kept.
        kept: bool,
    },
    /// Missing, not kept.
    MissingNotKept,
    /// Marked removable, deleted, not kept.
    RemovableDeletedNotKept,
    /// Fingerprint only (the owner's choice) and missing.
    FingerprintOnlyMissing,
}

impl Situation {
    fn not_checkable_reason(self) -> &'static str {
        match self {
            Situation::KeptOrMatches => "",
            Situation::DiskDiffers { .. } => "disk differs, bytes not kept",
            Situation::MissingNotKept => "missing, not kept",
            Situation::RemovableDeletedNotKept => "removable and deleted, not kept",
            Situation::FingerprintOnlyMissing => "fingerprint only and missing",
        }
    }
}

/// What the Core knows about a cited version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "five independent facts the Core reads from Files and the disk"
)]
pub struct VersionFacts {
    /// A file is at the version's path.
    pub on_disk: bool,
    /// That file's bytes match the pinned fingerprint.
    pub matches: bool,
    /// The pinned bytes are kept (`file.kept` without `fingerprint_only`).
    pub bytes_kept: bool,
    /// The owner kept only the fingerprint.
    pub fingerprint_only: bool,
    /// The identity is marked removable.
    pub removable: bool,
}

impl Situation {
    /// The 7.3 column, read as 7.3's paragraph under the table says: disk
    /// differs wins over kept; a kept version missing from disk is in the
    /// first column; removable wins over fingerprint only.
    #[must_use]
    pub fn of(f: VersionFacts) -> Situation {
        if f.on_disk {
            if f.matches {
                Situation::KeptOrMatches
            } else {
                Situation::DiskDiffers { kept: f.bytes_kept }
            }
        } else if f.bytes_kept {
            Situation::KeptOrMatches
        } else if f.removable {
            Situation::RemovableDeletedNotKept
        } else if f.fingerprint_only {
            Situation::FingerprintOnlyMissing
        } else {
            Situation::MissingNotKept
        }
    }
}

/// A cell's outcome before the pinned test.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Base {
    /// The cell says holds.
    Holds,
    /// The cell says `not checkable`.
    NotCheckable,
    /// Holds if the citation type's test passes on the pinned version,
    /// fails otherwise.
    Check,
}

/// One cell of 7.3: its outcome rule and what goes with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rule {
    /// The outcome rule.
    pub base: Base,
    /// Flag `evidence moved` (disk differs).
    pub evidence_moved: bool,
    /// Label `expected-missing`.
    pub expected_missing: bool,
    /// Flag `cited file retired`.
    pub cited_file_retired: bool,
}

const fn rule(base: Base, evidence_moved: bool, expected_missing: bool) -> Rule {
    Rule {
        base,
        evidence_moved,
        expected_missing,
        cited_file_retired: false,
    }
}

/// The 7.3 cell for a citation type, a situation, and whether the cited
/// identity is retired ("as the matching column, plus flag `cited file
/// retired`"; retirement arrives with gate C).
#[must_use]
#[allow(
    clippy::match_same_arms,
    reason = "one arm per 7.3 cell, so the match reads as the table"
)]
pub fn cell(t: CitationType, s: Situation, retired: bool) -> Rule {
    use Base::{Check, Holds, NotCheckable};
    use CitationType::{Cells, Page, Panel, Reserved, WholeFile};
    use Situation::{
        DiskDiffers, FingerprintOnlyMissing, KeptOrMatches, MissingNotKept, RemovableDeletedNotKept,
    };
    let r = match (t, s) {
        // Cells by key.
        (Cells, KeptOrMatches) => rule(Check, false, false),
        (Cells, DiskDiffers { kept: true }) => rule(Check, true, false),
        (Cells, DiskDiffers { kept: false }) => rule(NotCheckable, true, false),
        (Cells, MissingNotKept) => rule(NotCheckable, false, false),
        (Cells, RemovableDeletedNotKept) => rule(NotCheckable, false, true),
        (Cells, FingerprintOnlyMissing) => rule(NotCheckable, false, false),
        // Whole file: "Holds against the pinned version; flag".
        (WholeFile, KeptOrMatches) => rule(Holds, false, false),
        (WholeFile, DiskDiffers { .. }) => rule(Holds, true, false),
        (WholeFile, MissingNotKept) => rule(NotCheckable, false, false),
        (WholeFile, RemovableDeletedNotKept) => rule(NotCheckable, false, true),
        (WholeFile, FingerprintOnlyMissing) => rule(NotCheckable, false, false),
        // Page: disk differs "as cells"; the removable column carries no
        // label in 7.3's page row.
        (Page, KeptOrMatches) => rule(Check, false, false),
        (Page, DiskDiffers { kept: true }) => rule(Check, true, false),
        (Page, DiskDiffers { kept: false }) => rule(NotCheckable, true, false),
        (Page, MissingNotKept) => rule(NotCheckable, false, false),
        (Page, RemovableDeletedNotKept) => rule(NotCheckable, false, false),
        (Page, FingerprintOnlyMissing) => rule(NotCheckable, false, false),
        // Panel: disk differs "Holds (labels are metadata); flag".
        (Panel, KeptOrMatches) => rule(Check, false, false),
        (Panel, DiskDiffers { .. }) => rule(Holds, true, false),
        (Panel, MissingNotKept) => rule(NotCheckable, false, false),
        (Panel, RemovableDeletedNotKept) => rule(NotCheckable, false, true),
        (Panel, FingerprintOnlyMissing) => rule(NotCheckable, false, false),
        // Reserved: `not checkable` in every column, Retired included.
        (Reserved, _) => {
            return rule(NotCheckable, false, false);
        }
    };
    Rule {
        cited_file_retired: retired,
        ..r
    }
}

/// An evidence-exists outcome (11.16's `evidence.outcome`; `error` is the
/// Checker's and never comes from this table).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Outcome {
    /// Holds.
    Holds,
    /// Fails.
    Fails,
    /// Not checkable.
    NotCheckable,
}

impl Outcome {
    /// The 11.16 spelling.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Outcome::Holds => "holds",
            Outcome::Fails => "fails",
            Outcome::NotCheckable => "not_checkable",
        }
    }
}

/// One citation's result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CitationResult {
    /// The outcome.
    pub outcome: Outcome,
    /// The 7.3 reason, `None` for holds.
    pub reason: Option<&'static str>,
    /// The cell it came from.
    pub rule: Rule,
}

/// Evaluate one citation. `test` answers the citation type's test on the
/// pinned version and is called only when the cell needs it.
pub fn evaluate(
    t: CitationType,
    s: Situation,
    retired: bool,
    test: impl FnOnce() -> bool,
) -> CitationResult {
    let r = cell(t, s, retired);
    let (outcome, reason) = match r.base {
        Base::Holds => (Outcome::Holds, None),
        Base::Check if test() => (Outcome::Holds, None),
        Base::Check => (Outcome::Fails, Some(t.fails_reason())),
        Base::NotCheckable if t == CitationType::Reserved => {
            (Outcome::NotCheckable, Some("reserved citation kind"))
        }
        Base::NotCheckable => (Outcome::NotCheckable, Some(s.not_checkable_reason())),
    };
    CitationResult {
        outcome,
        reason,
        rule: r,
    }
}

/// A claim's `evidence`: `{"outcome","detail"}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Evidence {
    /// The combined outcome.
    pub outcome: Outcome,
    /// `None` for holds, else one clause per citation that did not hold.
    pub detail: Option<String>,
}

impl Evidence {
    /// Combine per-citation results, in citation order, each with its
    /// subject (the citation's `version`, or a reserved kind's `kind`), as
    /// 7.3 says: `fails` if any fails, else `not_checkable` if any is,
    /// else `holds`; no citations is `not_checkable`, with no clause to
    /// give, so `detail` is `null`.
    #[must_use]
    pub fn combine(results: &[(String, CitationResult)]) -> Evidence {
        let outcome = if results.is_empty() {
            Outcome::NotCheckable
        } else if results.iter().any(|(_, r)| r.outcome == Outcome::Fails) {
            Outcome::Fails
        } else if results
            .iter()
            .any(|(_, r)| r.outcome == Outcome::NotCheckable)
        {
            Outcome::NotCheckable
        } else {
            Outcome::Holds
        };
        let clauses: Vec<String> = results
            .iter()
            .enumerate()
            .filter(|(_, (_, r))| r.outcome != Outcome::Holds)
            .map(|(i, (subject, r))| format!("{i} {subject}: {}", r.reason.unwrap_or_default()))
            .collect();
        Evidence {
            outcome,
            detail: (!clauses.is_empty()).then(|| clauses.join("; ")),
        }
    }

    /// The 11.16 object.
    #[must_use]
    pub fn to_json(&self) -> Value {
        json!({"outcome": self.outcome.as_str(), "detail": self.detail})
    }
}

/// The subject 7.3's detail names for a citation: its version, or a
/// reserved kind's `kind`.
#[must_use]
pub fn subject(c: &tm_formats::citation::Citation) -> String {
    use tm_formats::citation::Citation;
    match c {
        Citation::Cells { version, .. }
        | Citation::WholeFile { version }
        | Citation::Page { version, .. }
        | Citation::Panel { version, .. } => version.to_string(),
        Citation::Reserved(v) => v
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
    }
}
