//! The read interface (section 10; item 78): one function per gate A
//! query, typed parameters and results, each with a `--json` rendering
//! (`to_json`). Queries read the index plus, for bytes and tables, Files,
//! and for `claims.evidence` and `checker.preview_steps`, the Checker.
//!
//! A flag is returned in one shape wherever it appears (section 10):
//! [`Flag`], with kind, subject, reason, and next command.

pub mod audit;
pub mod checker;
pub mod claims;
pub mod files;
pub mod flags;
pub mod reports;
pub mod settings;

use serde_json::{Value, json};

/// A flag or label as every query returns it (section 10).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Flag {
    /// The 5.6 name.
    pub name: String,
    /// Label or flag.
    pub label: bool,
    /// What it is on: `claim`, `span`, `file`, `version`.
    pub subject_kind: String,
    /// The subject: a ref, `T<n>:<line>:<pos>`, `F<n>`, `F<n>@v<m>`.
    pub subject: String,
    /// The reason shown.
    pub reason: String,
    /// The clearing action (5.6), `Permanent` for a label.
    pub next: String,
}

impl Flag {
    /// The JSON shape.
    #[must_use]
    pub fn to_json(&self) -> Value {
        json!({
            "kind": if self.label { "label" } else { "flag" },
            "name": self.name,
            "subject_kind": self.subject_kind,
            "subject": self.subject,
            "reason": self.reason,
            "next": self.next,
        })
    }
}

/// A list of JSON values, for results that are lists.
pub(crate) fn list(items: impl IntoIterator<Item = Value>) -> Value {
    Value::Array(items.into_iter().collect())
}
