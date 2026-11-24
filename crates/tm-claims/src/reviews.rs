//! Reviews (item 67; section 7; 5.4's review row): the `claim.reviewed`
//! event and the rule that a review never changes a verdict. A review is a
//! line of judgment beside the verdicts: the fold adds a `reviews` row,
//! moves `state` from `added` to `reviewed`, and clears `promoted, not
//! reviewed`; it never writes `verdicts` and never clears `assertion
//! fails` (item 70).

use serde_json::{Value, json};
use tm_formats::events::{EventType, KnownEvent};
use tm_formats::id::{ClaimRef, EventId};

use crate::claim::State;
use crate::{Code, Error, Result};

/// A review's outcome (11.16).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReviewOutcome {
    /// Confirmed.
    Confirmed,
    /// Disputed.
    Disputed,
    /// A question.
    Question,
}

impl ReviewOutcome {
    /// The 11.16 spelling.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            ReviewOutcome::Confirmed => "confirmed",
            ReviewOutcome::Disputed => "disputed",
            ReviewOutcome::Question => "question",
        }
    }

    /// Parse the 11.16 spelling.
    #[must_use]
    pub fn parse(s: &str) -> Option<ReviewOutcome> {
        match s {
            "confirmed" => Some(ReviewOutcome::Confirmed),
            "disputed" => Some(ReviewOutcome::Disputed),
            "question" => Some(ReviewOutcome::Question),
            _ => None,
        }
    }
}

/// One review as the owner gives it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Review {
    /// The reviewed claim.
    pub claim_ref: ClaimRef,
    /// The revision reviewed (the claim's current revision).
    pub revision: EventId,
    /// The outcome.
    pub outcome: ReviewOutcome,
    /// The note, if any.
    pub note: Option<String>,
    /// The registered report's fingerprint at review time (5.4).
    pub report_fingerprint: EventId,
}

impl Review {
    /// The `claim.reviewed` event. `state` is the claim's state from the
    /// index, `None` when the ref is unknown (section 9's `claim.review`
    /// row). Any state may be reviewed: a review is a line, not a
    /// transition the claim must allow.
    ///
    /// # Errors
    /// `E6201` for an unknown ref; `E1002` for a body not of its form.
    pub fn event(&self, state: Option<State>) -> Result<KnownEvent> {
        if state.is_none() {
            return Err(Error::code(
                Code::E6201,
                format!("no claim {}", self.claim_ref),
            ));
        }
        Ok(KnownEvent::new(
            EventType::ClaimReviewed,
            &json!({
                "ref": self.claim_ref.to_string(),
                "revision": self.revision.as_str(),
                "outcome": self.outcome.as_str(),
                "note": self.note.clone().map_or(Value::Null, Value::String),
                "report_fingerprint": self.report_fingerprint.as_str(),
            }),
        )?)
    }
}
