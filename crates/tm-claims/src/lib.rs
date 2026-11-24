//! Toulmin Claims library (design 4.1, sections 5.6, 7, 11.22, items 63
//! to 71): claims, refs, revisions, the 7.3 evidence-exists table, the 5.6
//! flags and labels, reviews, report-span scanning, drift, and the Claims
//! fold into the 11.17 tables.
//!
//! A library in the sense of 4.2: it calls no other library (rule 1). The
//! Core hands it Checker results, Files lookups, and report text, and it
//! returns events (as [`tm_formats::events::KnownEvent`]) for the Core to
//! batch; it never writes the record. Its fold writes only its own 11.17
//! tables, through [`tm_index::IndexTx`] (rule 6), and never `spans`,
//! which is scan-derived.

// The 11.12 error shape is fixed and errors are the cold path; boxing it
// would buy nothing measurable (as in tm-formats).
#![allow(clippy::result_large_err)]

use std::fmt;

pub use tm_formats::{Code, Error as FormatError};

pub mod claim;
pub mod drift;
pub mod evidence_exists;
pub mod flags;
pub mod fold;
pub mod refs;
pub mod reviews;
pub mod spans;

pub use crate::claim::{Claim, Kind, State};
pub use crate::drift::Drift;
pub use crate::evidence_exists::{CitationType, Outcome, Rule, Situation};
pub use crate::flags::{Name, NameKind};
pub use crate::spans::Span;

/// A Claims error.
#[derive(Debug)]
pub enum Error {
    /// A catalog error (`E6201`, `E1002`, ...) or a formats error.
    Format(FormatError),
    /// SQLite refused (the fold and the index reads).
    Index(tm_index::Error),
}

/// Alias for results in this crate.
pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub(crate) fn code(code: Code, message: impl Into<String>) -> Error {
        Error::Format(FormatError::new(code, message))
    }

    /// The catalog code, when this is a catalog error.
    #[must_use]
    pub fn catalog_code(&self) -> Option<Code> {
        match self {
            Error::Format(e) => Some(e.code),
            Error::Index(_) => None,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Format(e) => write!(f, "{e}"),
            Error::Index(e) => write!(f, "index: {e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<FormatError> for Error {
    fn from(e: FormatError) -> Self {
        Error::Format(e)
    }
}

impl From<tm_index::Error> for Error {
    fn from(e: tm_index::Error) -> Self {
        Error::Index(e)
    }
}
