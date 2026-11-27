//! Toulmin Core (design 4.1, items 72 to 81): every user-level operation,
//! all permission checks, batch assembly, the index and its folds, the
//! node-local stat cache, the read interface, settings, and `tm audit`.
//!
//! The Core is the only part that calls the libraries, and it passes data
//! between them (4.2 rule 1): a parsed Arrow table from Files into the
//! Checker, Checker results into Claims. Permissions are checked here and
//! nowhere else (rule 4): every owner operation takes an `&OwnerCap`, and
//! so does the main-chain append it ends in. The index fold writes through
//! `tm-index`; each library's fold writes its own 11.17 tables and the Core
//! writes `head`, `events`, `opaque_events`, `settings`, and `audits`
//! (rule 6). P7 builds the Core without the static view (item 79, P8).

// The 11.12 error shape is fixed and errors are the cold path, as in every
// other crate; boxing it would buy nothing measurable.
#![allow(clippy::result_large_err)]

pub mod actor;
pub mod audit;
pub mod batch;
pub mod index;
pub mod ops;
pub mod project;
pub mod read;
pub mod settings;
pub mod stat_cache;

use std::fmt;
use std::path::{Path, PathBuf};

pub use tm_formats::{Code, Error as FormatError};
pub use tm_owner_cap::OwnerCap;

pub use crate::ops::Core;
pub use crate::project::{Dirs, Project};
pub use crate::settings::Settings;

/// A Core error.
#[derive(Debug)]
pub enum Error {
    /// The operating system refused.
    Io {
        /// The path concerned.
        path: PathBuf,
        /// The cause.
        source: std::io::Error,
    },
    /// A catalog error (11.12).
    Format(FormatError),
    /// The Record library refused (`Error::Io` there is kept as such).
    Record(tm_record::Error),
    /// The Files library refused.
    Files(tm_files::Error),
    /// The Claims library refused.
    Claims(tm_claims::Error),
    /// SQLite refused.
    Index(tm_index::Error),
    /// `DuckDB` refused (the engine facts a verdict records).
    Engine(tm_duckdb::Error),
}

/// Alias for results in this crate.
pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Error {
        Error::Io {
            path: path.into(),
            source,
        }
    }

    pub(crate) fn code(code: Code, message: impl Into<String>) -> Error {
        Error::Format(FormatError::new(code, message))
    }

    pub(crate) fn at(code: Code, field: impl Into<String>, message: impl Into<String>) -> Error {
        Error::Format(FormatError::at(code, field, message))
    }

    /// The catalog code, when this is a catalog error at any depth.
    #[must_use]
    pub fn catalog_code(&self) -> Option<Code> {
        match self {
            Error::Format(e) => Some(e.code),
            Error::Record(e) => e.catalog_code(),
            Error::Files(e) => e.catalog_code(),
            Error::Claims(e) => e.catalog_code(),
            Error::Io { .. } | Error::Index(_) | Error::Engine(_) => None,
        }
    }

    /// The catalog error itself, when this is one at any depth.
    #[must_use]
    pub fn as_format(&self) -> Option<&FormatError> {
        match self {
            Error::Format(e)
            | Error::Record(tm_record::Error::Format(e))
            | Error::Files(tm_files::Error::Format(e))
            | Error::Claims(tm_claims::Error::Format(e)) => Some(e),
            _ => None,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io { path, source } => write!(f, "{}: {source}", path.display()),
            Error::Format(e) => write!(f, "{e}"),
            Error::Record(e) => write!(f, "record: {e}"),
            Error::Files(e) => write!(f, "files: {e}"),
            Error::Claims(e) => write!(f, "claims: {e}"),
            Error::Index(e) => write!(f, "index: {e}"),
            Error::Engine(e) => write!(f, "DuckDB: {e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<FormatError> for Error {
    fn from(e: FormatError) -> Self {
        Error::Format(e)
    }
}

impl From<tm_record::Error> for Error {
    fn from(e: tm_record::Error) -> Self {
        match e {
            tm_record::Error::Io { path, source } => Error::Io { path, source },
            other => Error::Record(other),
        }
    }
}

impl From<tm_files::Error> for Error {
    fn from(e: tm_files::Error) -> Self {
        match e {
            tm_files::Error::Io { path, source } => Error::Io { path, source },
            tm_files::Error::Index(e) => Error::Index(e),
            other => Error::Files(other),
        }
    }
}

impl From<tm_claims::Error> for Error {
    fn from(e: tm_claims::Error) -> Self {
        match e {
            tm_claims::Error::Index(e) => Error::Index(e),
            other => Error::Claims(other),
        }
    }
}

impl From<tm_index::Error> for Error {
    fn from(e: tm_index::Error) -> Self {
        Error::Index(e)
    }
}

impl From<tm_fsfacts::Error> for Error {
    fn from(e: tm_fsfacts::Error) -> Self {
        match e {
            tm_fsfacts::Error::Io { path, source } => Error::Io { path, source },
            tm_fsfacts::Error::Format(e) => Error::Format(e),
        }
    }
}

impl From<tm_duckdb::Error> for Error {
    fn from(e: tm_duckdb::Error) -> Self {
        Error::Engine(e)
    }
}

/// Read a whole file.
pub(crate) fn read_file(path: &Path) -> Result<Vec<u8>> {
    std::fs::read(path).map_err(|e| Error::io(path, e))
}

/// Read a whole UTF-8 text file.
pub(crate) fn read_text(path: &Path) -> Result<String> {
    std::fs::read_to_string(path).map_err(|e| Error::io(path, e))
}

/// The time now in the `at` form of 11.2.
#[must_use]
pub fn now_at() -> String {
    tm_record::now_at()
}
