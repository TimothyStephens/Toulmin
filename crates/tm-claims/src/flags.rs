//! The 5.6 flag and label names (item 66), each with its kind, subject,
//! and clearing action, and where gate A raises it: event-derived flags
//! the Claims fold stores in `flags`, labels read from the rows their
//! events fold into, and scan-derived flags computed at read time by the
//! Core's `read/flags.rs` (item 66, as split on 2026-11-24).

use crate::claim::State;

/// Label (permanent) or flag (has a clearing action), as 5.6 has it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NameKind {
    /// Permanent.
    Label,
    /// Has a clearing action.
    Flag,
}

/// Where gate A raises a name (item 66).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GateA {
    /// A flag the Claims fold stores in `flags`.
    ClaimsFold,
    /// A label read at read time from the rows its events fold into.
    FromRows,
    /// A flag computed at read time from the stat cache, the scanned
    /// `spans` table, and `report.check`.
    ScanDerived,
    /// Not in item 66's gate A list (another gate, or item 72's
    /// owner-action labels).
    NotListed,
}

macro_rules! names {
    ($( $v:ident => $s:literal, $k:ident, $on:literal, $clear:literal, $g:ident; )*) => {
        /// A 5.6 name.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum Name {
            $(
                #[doc = concat!("`", $s, "`")]
                $v,
            )*
        }

        impl Name {
            /// Every 5.6 name, in 5.6's order.
            pub const ALL: &[Name] = &[$(Name::$v),*];

            /// The name as 5.6 spells it, which is also `flags.name`.
            #[must_use]
            pub fn as_str(self) -> &'static str {
                match self { $(Name::$v => $s,)* }
            }

            /// Label or flag.
            #[must_use]
            pub fn kind(self) -> NameKind {
                match self { $(Name::$v => NameKind::$k,)* }
            }

            /// 5.6's "On" column.
            #[must_use]
            pub fn on(self) -> &'static str {
                match self { $(Name::$v => $on,)* }
            }

            /// 5.6's "Clearing action" column.
            #[must_use]
            pub fn clearing_action(self) -> &'static str {
                match self { $(Name::$v => $clear,)* }
            }

            /// Where gate A raises it (item 66).
            #[must_use]
            pub fn gate_a(self) -> GateA {
                match self { $(Name::$v => GateA::$g,)* }
            }
        }
    };
}

names! {
    Declared => "declared", Label, "Run", "Permanent", NotListed;
    CarriedForward => "carried forward", Label, "Output version", "Permanent", NotListed;
    MetadataOnly => "metadata only", Label, "Version", "Permanent", NotListed;
    ContentChanged => "content changed", Label, "Output version", "Permanent", NotListed;
    ChangedOutsideARun => "changed outside a run", Label, "Input version", "Permanent", NotListed;
    InputsChangedDuringRun => "inputs changed during run", Label, "Run", "Permanent", NotListed;
    StartedReceiptMissing => "started receipt missing", Label, "Run", "Permanent", NotListed;
    DeclarationChangedDuringRun => "declaration changed during run", Label, "Run and its output versions", "Permanent", NotListed;
    Interrupted => "interrupted", Flag, "Run", "`tm run finish`, or a new run", NotListed;
    RunInProgress => "run in progress", Flag, "Run", "Arrival of the finished receipt", NotListed;
    RunCheckFailed => "run check failed", Flag, "Run", "A new run, or `tm run review --note` acknowledging it", NotListed;
    ApprovalRequired => "approval required", Flag, "Code version, or one rerun", "`tm run approve`", NotListed;
    OwnerCodeNotSeparatelyReviewed => "owner code, not separately reviewed", Label, "Code version", "Permanent", NotListed;
    AttributionPartial => "attribution partial", Label, "Code version", "Permanent", NotListed;
    PossiblyChangedUnconfirmed => "possibly changed (unconfirmed)", Flag, "File version", "`tm status --refresh --hash`, or the next `tm run`", ScanDerived;
    Retired => "retired", Label, "File identity", "Permanent; `tm file restore` adds a `restored` label", NotListed;
    Restored => "restored", Label, "File identity", "Permanent", FromRows;
    Removable => "removable", Label, "File identity", "Permanent", FromRows;
    EvidenceNotKept => "evidence not kept", Label, "Claim", "Permanent", FromRows;
    ExpectedMissing => "expected-missing", Label, "File version", "Permanent", ScanDerived;
    RootNotDeclared => "root not declared", Label, "Lineage edge", "Permanent", NotListed;
    OwnCodeChanged => "own code changed", Flag, "Analysis, inherited by claims", "A successful rerun, or identical content upstream", NotListed;
    OwnInputsChanged => "own inputs changed", Flag, "Analysis, inherited by claims", "A successful rerun, or identical content upstream", NotListed;
    CommandChanged => "command changed", Flag, "Analysis, inherited by claims", "A successful rerun, or identical content upstream", NotListed;
    UpstreamChanged => "upstream changed", Flag, "Analysis, inherited by claims", "A successful rerun, or identical content upstream", NotListed;
    EvidenceMoved => "evidence moved", Flag, "Claim", "`tm claim accept`, a rerun, or restoring the bytes", ScanDerived;
    CitedFileRetired => "cited file retired", Flag, "Claim", "`tm file restore`, or supersede or retract the claim", NotListed;
    AssertionFails => "assertion fails", Flag, "Claim", "A revision, a supersede, or a retract", ClaimsFold;
    TextDrifted => "text drifted", Flag, "Claim", "`tm claim update-text`, or editing the span back", ScanDerived;
    PointsAtARetractedClaim => "points at a retracted claim", Flag, "Span", "Edit the span", ScanDerived;
    PointsAtASupersededClaim => "points at a superseded claim", Flag, "Span", "Edit the span", ScanDerived;
    PromotedNotReviewed => "promoted, not reviewed", Flag, "Claim", "Any review", ClaimsFold;
    PromotedWithUnapprovedCode => "promoted with unapproved code", Label, "Claim", "Permanent", NotListed;
    EvidenceChangedSinceDrafting => "evidence changed since drafting", Flag, "Draft claim", "None while the draft is open: promotion is refused with `E6505`", NotListed;
    Unisolated => "unisolated", Label, "Session and its work", "Permanent", NotListed;
    IsolationUnknown => "isolation unknown", Label, "Session and its work", "Permanent", NotListed;
    PossiblyAgentIssued => "possibly agent-issued", Label, "Owner action or receipt", "Permanent", NotListed;
    DuringUnisolatedSession => "during unisolated session", Label, "Owner action", "Permanent", NotListed;
    PendingViewerAction => "pending viewer action", Flag, "Viewer journal event", "`tm apply --viewer`, or `tm status` after an interactive confirmation (3.2)", NotListed;
    Rejected => "rejected", Label, "Draft item", "Permanent; a later revision is a new event", NotListed;
    Retracted => "retracted", Label, "Claim", "Permanent", FromRows;
    Superseded => "superseded", Label, "Claim", "Permanent", FromRows;
}

impl Name {
    /// Look a name up by its 5.6 spelling.
    #[must_use]
    pub fn parse(s: &str) -> Option<Name> {
        Name::ALL.iter().copied().find(|n| n.as_str() == s)
    }
}

/// `flags.subject_kind` for a claim (item 70).
pub const SUBJECT_CLAIM: &str = "claim";

/// The reason the fold stores with `promoted, not reviewed` (5.6's
/// "Reason shown").
pub const REASON_NOT_REVIEWED: &str = "No review yet";

/// The reason the fold stores with `assertion fails` (item 70).
#[must_use]
pub fn assertion_fails_reason(against: &[String]) -> String {
    format!("the assertion check failed against {}", against.join(", "))
}

/// The scan-derived span flag for a span whose ref is in `state`, if any
/// (5.6: `points at a retracted claim`, `points at a superseded claim`).
#[must_use]
pub fn span_flag(state: State) -> Option<Name> {
    match state {
        State::Retracted => Some(Name::PointsAtARetractedClaim),
        State::Superseded => Some(Name::PointsAtASupersededClaim),
        State::Added | State::Reviewed => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_66_split() {
        let of = |g: GateA| -> Vec<&str> {
            Name::ALL
                .iter()
                .filter(|n| n.gate_a() == g)
                .map(|n| n.as_str())
                .collect()
        };
        assert_eq!(
            of(GateA::ClaimsFold),
            ["assertion fails", "promoted, not reviewed"]
        );
        assert_eq!(
            of(GateA::FromRows),
            [
                "restored",
                "removable",
                "evidence not kept",
                "retracted",
                "superseded"
            ]
        );
        assert_eq!(
            of(GateA::ScanDerived),
            [
                "possibly changed (unconfirmed)",
                "expected-missing",
                "evidence moved",
                "text drifted",
                "points at a retracted claim",
                "points at a superseded claim"
            ]
        );
        for n in Name::ALL {
            match n.gate_a() {
                GateA::ClaimsFold => assert_eq!(n.kind(), NameKind::Flag),
                GateA::FromRows => assert_eq!(n.kind(), NameKind::Label),
                _ => {}
            }
            assert_eq!(Name::parse(n.as_str()), Some(*n));
        }
        assert_eq!(Name::ALL.len(), 42);
    }

    #[test]
    fn span_flags() {
        assert_eq!(
            span_flag(State::Retracted),
            Some(Name::PointsAtARetractedClaim)
        );
        assert_eq!(span_flag(State::Reviewed), None);
    }
}
