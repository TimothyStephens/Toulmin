//! The Checker's verdict (design item 58): the parts of the 11.16
//! `claim.checked` body the Checker produces. `ref`, `revision`,
//! `against`, and `evidence` are the Claims library's (section 9's
//! `claim.add` row) and are not here.

use std::collections::BTreeMap;

use serde_json::{Map, Value, json};

use tm_formats::Code;
use tm_formats::errors::Error;
use tm_formats::typed_value::TypedValue;

/// A Checker verdict: `holds`, `fails`, or `error` (11.18.3 Verdicts).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The assertion holds.
    Holds,
    /// The assertion fails.
    Fails,
    /// The check could not complete; `Verdict::error` says why.
    Error,
}

impl Outcome {
    /// The 11.16 spelling.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Outcome::Holds => "holds",
            Outcome::Fails => "fails",
            Outcome::Error => "error",
        }
    }
}

/// One `steps` entry of the body: `{"op","rows_out","description"}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepSummary {
    /// The 11.8 op name.
    pub op: &'static str,
    /// Rows out of the step.
    pub rows_out: u64,
    /// The 11.18.4 sentence.
    pub description: String,
}

/// The body's `assertion`: `{"outcome","observed","expected"}`.
#[derive(Debug, Clone, PartialEq)]
pub struct AssertionResult {
    /// `holds`, `fails`, or `error`.
    pub outcome: Outcome,
    /// What the Checker located, in the form `crates/tm-formats/README.md`
    /// gives for evaluation cases: `{"cell"}`, `{"position"}`, `{"count"}`,
    /// `{"left","right"}`, or `{"present"}`, `null` for an absent row.
    pub observed: Value,
    /// The assertion's 11.9 `expected` (a typed value or `[lo, hi]`), or
    /// `null` for a kind without one (`compare_rows`, `exists`).
    pub expected: Value,
}

/// The body's `versions`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Versions {
    /// Version handle to parse-spec fingerprint.
    pub parse_specs: BTreeMap<String, String>,
    /// [`crate::translate::TRANSLATOR_VERSION`].
    pub translator: String,
    /// `DuckDB`'s version.
    pub duckdb: String,
    /// Loaded extensions.
    pub extensions: Vec<String>,
    /// [`crate::rules::RULES_VERSION`].
    pub rules: String,
    /// `musl` or `glibc`.
    pub build: String,
}

/// The error of a verdict `error`: the 11.12 code, the JSON Pointer of the
/// offending member of the steps or assertion (when there is one), and a
/// message. The body records `{"code","message"}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckError {
    /// The code.
    pub code: Code,
    /// The pointer into the steps or the assertion.
    pub field: Option<String>,
    /// A message for people.
    pub message: String,
}

impl From<Error> for CheckError {
    fn from(e: Error) -> Self {
        CheckError {
            code: e.code,
            field: e.field,
            message: e.message,
        }
    }
}

/// A verdict.
#[derive(Debug, Clone, PartialEq)]
pub struct Verdict {
    /// The Checker verdict.
    pub outcome: Outcome,
    /// The assertion's result; `None` when the check stopped before the
    /// assertion was evaluated.
    pub assertion: Option<AssertionResult>,
    /// One entry per op, in order; empty on `error`.
    pub steps: Vec<StepSummary>,
    /// `final_rows_hash` (11.18.2 Result hash); `None` on `error`.
    pub final_rows_hash: Option<String>,
    /// `key_map_hash` (11.18.2 Key mapping); `None` on `error`.
    pub key_map_hash: Option<String>,
    /// The key map in final-row order; empty on `error`.
    pub key_map: Vec<Vec<TypedValue>>,
    /// The SQL that ran, in order.
    pub sql: String,
    /// What ran.
    pub versions: Versions,
    /// Why the verdict is `error`.
    pub error: Option<CheckError>,
}

impl Verdict {
    /// The Checker's members of the `claim.checked` body: `assertion`,
    /// `steps`, `final_rows_hash`, `key_map_hash`, `key_map`, `sql`,
    /// `versions`, and `error`. A hash the Checker did not compute (verdict
    /// `error`) is `null`; 11.16 gives it no form for that case, which the
    /// Claims library settles when it writes the body (P6).
    #[must_use]
    pub fn body_members(&self) -> Map<String, Value> {
        let mut m = Map::new();
        m.insert(
            "assertion".into(),
            self.assertion.as_ref().map_or(Value::Null, |a| {
                json!({"outcome": a.outcome.as_str(), "observed": a.observed, "expected": a.expected})
            }),
        );
        m.insert(
            "steps".into(),
            Value::Array(
                self.steps
                    .iter()
                    .map(|s| json!({"op": s.op, "rows_out": s.rows_out, "description": s.description}))
                    .collect(),
            ),
        );
        m.insert(
            "final_rows_hash".into(),
            self.final_rows_hash
                .clone()
                .map_or(Value::Null, Value::String),
        );
        m.insert(
            "key_map_hash".into(),
            self.key_map_hash.clone().map_or(Value::Null, Value::String),
        );
        m.insert(
            "key_map".into(),
            Value::Array(
                self.key_map
                    .iter()
                    .map(|t| Value::Array(t.iter().map(TypedValue::to_json).collect()))
                    .collect(),
            ),
        );
        m.insert("sql".into(), Value::String(self.sql.clone()));
        m.insert(
            "versions".into(),
            json!({
                "parse_specs": self.versions.parse_specs,
                "translator": self.versions.translator,
                "duckdb": self.versions.duckdb,
                "extensions": self.versions.extensions,
                "rules": self.versions.rules,
                "build": self.versions.build,
            }),
        );
        m.insert(
            "error".into(),
            self.error.as_ref().map_or(
                Value::Null,
                |e| json!({"code": e.code.as_str(), "message": e.message}),
            ),
        );
        m
    }
}
