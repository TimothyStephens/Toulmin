//! Toulmin Checker (design 4.1, items 53 to 62): query steps and an
//! assertion over cited tables, evaluated to a verdict. Expressions run in
//! Rust under the 11.6 decimal context; the relational steps run in
//! `DuckDB` as explicit SQL; every step is described by its 11.18.4
//! template; the final rows and key map are hashed (11.18.2).
//!
//! A library: it never calls another library (4.2). The Core hands it the
//! cited tables as Arrow with their parse specs ([`InputTable`]).

// The 11.12 error shape is tm-formats' and errors are the cold path, as
// there; boxing it would buy nothing measurable.
#![allow(clippy::result_large_err)]

pub mod describe;
pub mod engine;
pub mod eval;
pub mod rules;
pub mod run;
pub mod table;
pub mod translate;
pub mod verdict;

pub use run::{InputTable, NamedRow, Request, Role, RowBeyondCap, RunResult, StepTable, run};
pub use verdict::{AssertionResult, CheckError, Outcome, StepSummary, Verdict, Versions};
