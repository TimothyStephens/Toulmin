//! Toulmin Record library (design 3, 11.3, 11.21; items 32 to 42).
//!
//! The main chain in framed, sealed, packed segments; `HEAD`; the `O_EXCL`
//! lock with epochs and fencing; the content-addressed object store; event
//! scans by position; and integrity checks. The journal and inbox APIs are
//! gate B and C. The Record stores nothing derived and never reads the
//! index (rule 6).

#![allow(clippy::result_large_err, reason = "errors are the cold path")]

pub mod append;
pub mod head;
pub mod layout;
pub mod lock;
pub mod objects;
pub mod scan;
pub mod segment;
pub mod verify;

use std::fmt;
use std::path::{Path, PathBuf};

pub use tm_formats::{Code, Error as FormatError};

pub use crate::append::Batch;
pub use crate::head::Head;
pub use crate::layout::RecordHeader;
pub use crate::lock::{Lock, LockHolder, LockOptions, TornTail};
pub use crate::scan::{Position, Scan, StoredEvent};
pub use crate::verify::{Finding, FindingKind};

/// A Record error.
#[derive(Debug)]
pub enum Error {
    /// The operating system refused.
    Io {
        /// The path concerned.
        path: PathBuf,
        /// The cause.
        source: std::io::Error,
    },
    /// A catalog error: `E2001` to `E2201`, `E3203`, or a formats error.
    Format(FormatError),
    /// `record.json` names a format this binary does not know; the record
    /// opens for reading and refuses every write (NFR Rollout).
    NewerFormat {
        /// The record's format.
        format: u64,
        /// The highest this binary writes.
        supported: u64,
    },
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

    /// The catalog code, when this is a catalog error.
    #[must_use]
    pub fn catalog_code(&self) -> Option<Code> {
        match self {
            Error::Format(e) => Some(e.code),
            Error::Io { .. } | Error::NewerFormat { .. } => None,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io { path, source } => write!(f, "{}: {source}", path.display()),
            Error::Format(e) => write!(f, "{e}"),
            Error::NewerFormat { format, supported } => write!(
                f,
                "record format {format} is newer than this binary's {supported}; a newer tm is needed to write"
            ),
        }
    }
}

impl std::error::Error for Error {}

impl From<FormatError> for Error {
    fn from(e: FormatError) -> Self {
        Error::Format(e)
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

/// An open record directory.
#[derive(Debug)]
pub struct Record {
    root: PathBuf,
    header: RecordHeader,
    segment_limit: u64,
    read_only: bool,
}

impl Record {
    /// The record directory.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// `main/`.
    #[must_use]
    pub fn main_dir(&self) -> PathBuf {
        self.root.join("main")
    }

    /// `objects/`.
    #[must_use]
    pub fn objects_dir(&self) -> PathBuf {
        self.root.join("objects")
    }

    /// `record.json`.
    #[must_use]
    pub fn header(&self) -> &RecordHeader {
        &self.header
    }

    /// The main chain's genesis string (11.2).
    #[must_use]
    pub fn genesis(&self) -> tm_formats::id::EventId {
        tm_formats::id::genesis("main", &self.header.project_id)
    }

    /// True when `record.json` names a newer format than this binary writes.
    #[must_use]
    pub fn is_read_only(&self) -> bool {
        self.read_only
    }

    /// The segment limit this writer seals at (11.3: 64 MiB).
    #[must_use]
    pub fn segment_limit(&self) -> u64 {
        self.segment_limit
    }

    /// Use another segment limit for this writer. Readers do not depend on
    /// it; item 40's conformance fixtures are generated with a small one.
    #[must_use]
    pub fn with_segment_limit(mut self, bytes: u64) -> Record {
        self.segment_limit = bytes.max(1);
        self
    }

    pub(crate) fn writable(&self) -> Result<()> {
        if self.read_only {
            return Err(Error::NewerFormat {
                format: self.header.format,
                supported: tm_formats::version::RECORD_FORMAT,
            });
        }
        Ok(())
    }
}

/// Write `value` as canonical JSON plus one newline at `path` (11.3's small
/// files), creating or truncating.
pub(crate) fn write_small_json(path: &Path, value: &serde_json::Value) -> Result<()> {
    use std::io::Write as _;
    let mut text = tm_formats::canonical::canonical(value)?;
    text.push('\n');
    let mut f = std::fs::File::create(path).map_err(|e| Error::io(path, e))?;
    f.write_all(text.as_bytes())
        .map_err(|e| Error::io(path, e))?;
    f.sync_all().map_err(|e| Error::io(path, e))?;
    Ok(())
}

/// Read a small JSON file written by [`write_small_json`].
pub(crate) fn read_small_json(path: &Path) -> Result<serde_json::Value> {
    let bytes = std::fs::read(path).map_err(|e| Error::io(path, e))?;
    Ok(tm_formats::canonical::parse_bytes(&bytes)?)
}

/// `fsync` a directory.
pub(crate) fn fsync_dir(dir: &Path) -> Result<()> {
    std::fs::File::open(dir)
        .and_then(|d| d.sync_all())
        .map_err(|e| Error::io(dir, e))
}

/// A fresh open after an `opendir` and `closedir` of `dir` (3.1): most
/// network clients drop their directory cache on it.
pub(crate) fn refresh_dir(dir: &Path) {
    if let Ok(rd) = std::fs::read_dir(dir) {
        for _ in rd {}
    }
}

/// The time now in the `at` form of 11.2.
#[must_use]
pub fn now_at() -> String {
    tm_fsfacts::time::Time::now().rfc3339_seconds()
}
