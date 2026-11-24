//! Claims (item 63; sections 7.1 and 7.2; 11.16): fields, kinds, states,
//! revisions, the current-revision rule for owner claims, and the events
//! an owner claim is recorded with. The cross-journal rule of D29 arrives
//! with gate B.

use serde_json::{Map, Value, json};
use tm_formats::citation::Citation;
use tm_formats::events::{EventType, KnownEvent};
use tm_formats::id::{ClaimRef, EventId, ReportHandle, VersionHandle};
use tm_formats::sort_order::cmp_text;

use crate::Result;
use crate::evidence_exists::Evidence;

/// A claim's kind (7.2; 11.16's `kind`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    /// Evidence-exists and assertion checks.
    Quantitative,
    /// Evidence-exists check only; the review is the only judgment.
    Qualitative,
    /// Evidence-exists check, and the assertion check for its quantitative
    /// part.
    Mixed,
}

impl Kind {
    /// The 11.16 spelling.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Quantitative => "quantitative",
            Kind::Qualitative => "qualitative",
            Kind::Mixed => "mixed",
        }
    }

    /// Parse the 11.16 spelling.
    #[must_use]
    pub fn parse(s: &str) -> Option<Kind> {
        match s {
            "quantitative" => Some(Kind::Quantitative),
            "qualitative" => Some(Kind::Qualitative),
            "mixed" => Some(Kind::Mixed),
            _ => None,
        }
    }

    /// True when 7.2 gives the kind an assertion check. A qualitative claim
    /// shows "no system check" instead.
    #[must_use]
    pub fn has_assertion_check(self) -> bool {
        !matches!(self, Kind::Qualitative)
    }
}

/// A claim's state in gate A (11.17's `claims.state`; 7.1 with the owner
/// claim's `added` standing for `promoted`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum State {
    /// Added by the owner, not yet reviewed.
    Added,
    /// At least one review line.
    Reviewed,
    /// Retracted by the owner; final.
    Retracted,
    /// Superseded by a new claim; final.
    Superseded,
}

impl State {
    /// The 11.17 spelling.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            State::Added => "added",
            State::Reviewed => "reviewed",
            State::Retracted => "retracted",
            State::Superseded => "superseded",
        }
    }

    /// Parse the 11.17 spelling.
    #[must_use]
    pub fn parse(s: &str) -> Option<State> {
        match s {
            "added" => Some(State::Added),
            "reviewed" => Some(State::Reviewed),
            "retracted" => Some(State::Retracted),
            "superseded" => Some(State::Superseded),
            _ => None,
        }
    }

    /// True for `retracted` and `superseded`, which no later event leaves
    /// (item 70).
    #[must_use]
    pub fn is_final(self) -> bool {
        matches!(self, State::Retracted | State::Superseded)
    }

    /// The state after a `claim.reviewed`: `reviewed` only from `added`.
    #[must_use]
    pub fn after_review(self) -> State {
        match self {
            State::Added => State::Reviewed,
            s => s,
        }
    }

    /// The state after a `claim.retracted`: from `added` or `reviewed`.
    #[must_use]
    pub fn after_retract(self) -> State {
        if self.is_final() {
            self
        } else {
            State::Retracted
        }
    }

    /// The state after a `claim.superseded`: from `added` or `reviewed`.
    #[must_use]
    pub fn after_supersede(self) -> State {
        if self.is_final() {
            self
        } else {
            State::Superseded
        }
    }
}

/// The content every revision restates in full (11.16, decision 12 of
/// `decisions-p1.md`): what `claim.added` and `claim.revised` share.
#[derive(Debug, Clone, PartialEq)]
pub struct Content {
    /// The recorded sentence.
    pub sentence: String,
    /// The cited version handles.
    pub cited: Vec<VersionHandle>,
    /// The 11.23 citation objects.
    pub citations: Vec<Citation>,
    /// The 11.8 steps object, or `None`.
    pub steps: Option<Value>,
    /// The 11.9 assertion object, or `None`.
    pub assertion: Option<Value>,
    /// The qualifiers, or `None`.
    pub qualifiers: Option<String>,
}

impl Content {
    fn insert_into(&self, m: &mut Map<String, Value>) {
        m.insert("sentence".into(), self.sentence.clone().into());
        m.insert(
            "cited".into(),
            Value::Array(
                self.cited
                    .iter()
                    .map(|v| Value::String(v.to_string()))
                    .collect(),
            ),
        );
        m.insert(
            "citations".into(),
            Value::Array(self.citations.iter().map(Citation::to_json).collect()),
        );
        m.insert("steps".into(), self.steps.clone().unwrap_or(Value::Null));
        m.insert(
            "assertion".into(),
            self.assertion.clone().unwrap_or(Value::Null),
        );
        m.insert(
            "qualifiers".into(),
            self.qualifiers.clone().map_or(Value::Null, Value::String),
        );
    }

    /// Read the shared keys from a validated `claim.added` or
    /// `claim.revised` body.
    #[must_use]
    pub fn from_body(body: &KnownEvent) -> Content {
        let cited = body
            .get("cited")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().and_then(VersionHandle::parse))
                    .collect()
            })
            .unwrap_or_default();
        let citations = body
            .get("citations")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .enumerate()
                    .filter_map(|(i, v)| {
                        Citation::from_json(v, &format!("/body/citations/{i}")).ok()
                    })
                    .collect()
            })
            .unwrap_or_default();
        let opt = |k: &str| body.get(k).filter(|v| !v.is_null()).cloned();
        Content {
            sentence: body
                .get("sentence")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            cited,
            citations,
            steps: opt("steps"),
            assertion: opt("assertion"),
            qualifiers: body
                .get("qualifiers")
                .and_then(Value::as_str)
                .map(str::to_owned),
        }
    }
}

/// An owner claim as `claim.added` records it.
#[derive(Debug, Clone, PartialEq)]
#[allow(
    clippy::struct_field_names,
    reason = "`ref` is a keyword; every type here names it `claim_ref`"
)]
pub struct Claim {
    /// The ref (`C<n>` in gate A).
    pub claim_ref: ClaimRef,
    /// The kind.
    pub kind: Kind,
    /// The report the claim's span is in.
    pub report: ReportHandle,
    /// The ref this claim supersedes, if any.
    pub supersedes: Option<ClaimRef>,
    /// The first revision's content.
    pub content: Content,
}

impl Claim {
    /// The `claim.added` event.
    ///
    /// # Errors
    /// `E1002` when a value is not of its 11.16 form (an empty sentence,
    /// an invalid steps or assertion object).
    pub fn added_event(&self) -> Result<KnownEvent> {
        let mut m = Map::new();
        m.insert("ref".into(), self.claim_ref.to_string().into());
        m.insert("kind".into(), self.kind.as_str().into());
        m.insert("report".into(), self.report.to_string().into());
        m.insert(
            "supersedes".into(),
            self.supersedes
                .map_or(Value::Null, |r| Value::String(r.to_string())),
        );
        self.content.insert_into(&mut m);
        Ok(KnownEvent::new(EventType::ClaimAdded, &Value::Object(m))?)
    }

    /// Read a validated `claim.added` body.
    #[must_use]
    pub fn from_body(body: &KnownEvent) -> Option<Claim> {
        let s = |k: &str| body.get(k).and_then(Value::as_str);
        Some(Claim {
            claim_ref: ClaimRef::parse(s("ref")?)?,
            kind: Kind::parse(s("kind")?)?,
            report: ReportHandle::parse(s("report")?)?,
            supersedes: s("supersedes").and_then(ClaimRef::parse),
            content: Content::from_body(body),
        })
    }
}

/// One revision: the event that created it and its content.
#[derive(Debug, Clone, PartialEq)]
pub struct Revision {
    /// The `claim.added` or `claim.revised` event's ID.
    pub event_id: EventId,
    /// The content it states.
    pub content: Content,
}

/// The current revision of an owner claim (7.1): every owner revision is
/// promoted when written, so it is the latest in chain order. `revisions`
/// is in chain order, `claim.added` first.
#[must_use]
pub fn current_revision(revisions: &[Revision]) -> Option<&Revision> {
    revisions.last()
}

/// The `claim.revised` event (`reason` `update_text`, the one gate A
/// reason).
///
/// # Errors
/// `E1002` when a value is not of its 11.16 form.
pub fn revised_event(claim_ref: ClaimRef, content: &Content) -> Result<KnownEvent> {
    let mut m = Map::new();
    m.insert("ref".into(), claim_ref.to_string().into());
    content.insert_into(&mut m);
    m.insert("reason".into(), "update_text".into());
    Ok(KnownEvent::new(EventType::ClaimRevised, &Value::Object(m))?)
}

/// The `claim.retracted` event.
///
/// # Errors
/// `E1002` for an empty reason.
pub fn retracted_event(claim_ref: ClaimRef, reason: &str) -> Result<KnownEvent> {
    Ok(KnownEvent::new(
        EventType::ClaimRetracted,
        &json!({"ref": claim_ref.to_string(), "reason": reason}),
    )?)
}

/// The `claim.superseded` event, written in the batch of the
/// replacement's `claim.added` (11.16).
///
/// # Errors
/// Never in practice; the body check runs as for every event.
pub fn superseded_event(claim_ref: ClaimRef, replacement: ClaimRef) -> Result<KnownEvent> {
    Ok(KnownEvent::new(
        EventType::ClaimSuperseded,
        &json!({"ref": claim_ref.to_string(), "replacement": replacement.to_string()}),
    )?)
}

/// `against` in 11.16's order: the version the key map's tuples come
/// from first, when the check has a final table, then the others in 11.10
/// order, each once.
#[must_use]
pub fn against(key_map_version: Option<VersionHandle>, others: &[VersionHandle]) -> Vec<String> {
    let mut rest: Vec<String> = others
        .iter()
        .filter(|v| Some(**v) != key_map_version)
        .map(ToString::to_string)
        .collect();
    rest.sort_by(|a, b| cmp_text(a, b));
    rest.dedup();
    let mut out: Vec<String> = key_map_version.iter().map(ToString::to_string).collect();
    out.extend(rest);
    out
}

/// The `claim.checked` event. `ref`, `revision`, `against`, and
/// `evidence` are the Claims library's; `checker` holds the Checker's
/// part (`assertion`, `steps`, `final_rows_hash`, `key_map_hash`,
/// `key_map`, `sql`, `versions`, `error`) as the Core received it. What
/// that part holds for a claim the Checker does not run is not settled
/// (runbook section 6) and is taken from the caller as given.
///
/// # Errors
/// `E1002` when the merged body is not of its 11.16 form.
pub fn checked_event(
    claim_ref: ClaimRef,
    revision: &EventId,
    against: &[String],
    evidence: &Evidence,
    checker: &Map<String, Value>,
) -> Result<KnownEvent> {
    let mut m = checker.clone();
    m.insert("ref".into(), claim_ref.to_string().into());
    m.insert("revision".into(), revision.as_str().into());
    m.insert(
        "against".into(),
        Value::Array(against.iter().cloned().map(Value::String).collect()),
    );
    m.insert("evidence".into(), evidence.to_json());
    Ok(KnownEvent::new(EventType::ClaimChecked, &Value::Object(m))?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> VersionHandle {
        VersionHandle::parse(s).unwrap()
    }

    #[test]
    fn against_order() {
        assert_eq!(
            against(Some(v("F3@v1")), &[v("F10@v1"), v("F3@v1"), v("F2@v1")]),
            vec!["F3@v1", "F10@v1", "F2@v1"]
        );
        assert_eq!(
            against(None, &[v("F2@v1"), v("F1@v1")]),
            vec!["F1@v1", "F2@v1"]
        );
        assert_eq!(against(Some(v("F1@v2")), &[]), vec!["F1@v2"]);
    }

    #[test]
    fn states() {
        assert_eq!(State::Added.after_review(), State::Reviewed);
        assert_eq!(State::Retracted.after_review(), State::Retracted);
        assert_eq!(State::Reviewed.after_retract(), State::Retracted);
        assert_eq!(State::Superseded.after_retract(), State::Superseded);
        assert_eq!(State::Reviewed.after_supersede(), State::Superseded);
        for s in ["added", "reviewed", "retracted", "superseded"] {
            assert_eq!(State::parse(s).unwrap().as_str(), s);
        }
        assert!(!Kind::Qualitative.has_assertion_check());
        assert!(Kind::Mixed.has_assertion_check());
    }

    #[test]
    fn current_is_latest() {
        let content = Content {
            sentence: "s".into(),
            cited: vec![],
            citations: vec![],
            steps: None,
            assertion: None,
            qualifiers: None,
        };
        let a = Revision {
            event_id: EventId::parse(&format!("sha256:{}", "a".repeat(64))).unwrap(),
            content: content.clone(),
        };
        let b = Revision {
            event_id: EventId::parse(&format!("sha256:{}", "b".repeat(64))).unwrap(),
            content,
        };
        assert_eq!(current_revision(&[a, b.clone()]), Some(&b));
        assert_eq!(current_revision(&[]), None);
    }
}
