//! Toulmin filesystem facts (design 4.1, D19, D25, items 26 to 31).
//!
//! Pure and stateless: paths, exclude lists, settings, and previous
//! snapshots in; snapshots, fingerprints, manifests, and "needs re-hash"
//! decisions out. Nothing here touches the record directory.

// The 11.12 error shape is fixed and errors are the cold path; boxing it
// would buy nothing measurable (as in tm-formats).
#![allow(clippy::result_large_err)]

pub mod hash;
pub mod manifest;
pub mod needs_rehash;
pub mod probes;
pub mod racy;
pub mod snapshot;
pub mod time;

use std::fmt;
use std::path::PathBuf;

/// A filesystem-facts error: an I/O failure at a path, or a formats error
/// (`E3003` for a non-UTF-8 path).
#[derive(Debug)]
pub enum Error {
    /// The operating system refused.
    Io {
        /// The path concerned.
        path: PathBuf,
        /// The cause.
        source: std::io::Error,
    },
    /// A catalog error (`E3003`, or a formats error from a snapshot read).
    Format(tm_formats::Error),
}

impl Error {
    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Error {
        Error::Io {
            path: path.into(),
            source,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io { path, source } => write!(f, "{}: {source}", path.display()),
            Error::Format(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<tm_formats::Error> for Error {
    fn from(e: tm_formats::Error) -> Self {
        Error::Format(e)
    }
}
