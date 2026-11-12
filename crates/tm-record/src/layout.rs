//! Creation and opening of the record directory (item 32; 11.3; 11.21;
//! glossary, Record directory): `record.json`, `main/` with its initial
//! `HEAD`, `objects/sha256/`, `objects/tmp/`. `journals/` and `inbox/` are
//! gates B and C.

use std::path::Path;

use serde_json::{Value, json};
use tm_formats::id::EventId;
use tm_formats::version::RECORD_FORMAT;

use crate::head::Head;
use crate::segment::SEGMENT_LIMIT;
use crate::{Code, Error, Record, Result, fsync_dir, read_small_json, write_small_json};

/// `record.json` (11.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordHeader {
    /// The record format version.
    pub format: u64,
    /// The project ID.
    pub project_id: String,
    /// The owner ID.
    pub owner_id: String,
    /// When the record was created, in the `at` form.
    pub created: String,
    /// `sha256`.
    pub hash_alg: String,
}

impl RecordHeader {
    /// The JSON object.
    #[must_use]
    pub fn to_json(&self) -> Value {
        json!({
            "format": self.format,
            "project_id": self.project_id,
            "owner_id": self.owner_id,
            "created": self.created,
            "hash_alg": self.hash_alg,
        })
    }

    fn from_json(v: &Value) -> Option<RecordHeader> {
        let m = v.as_object()?;
        if m.len() != 5 {
            return None;
        }
        let s = |k: &str| m.get(k)?.as_str().map(str::to_owned);
        Some(RecordHeader {
            format: m.get("format")?.as_u64()?,
            project_id: s("project_id")?,
            owner_id: s("owner_id")?,
            created: s("created")?,
            hash_alg: s("hash_alg")?,
        })
    }
}

impl Record {
    /// Create a record directory at `root` (section 9 `project.init`:
    /// layout, header, `HEAD`).
    ///
    /// # Errors
    /// `E2001` when `root` already holds a record or is not empty;
    /// `Error::Io` otherwise.
    pub fn create(root: &Path, project_id: &str, owner_id: &str, created: &str) -> Result<Record> {
        if root.join("record.json").exists() {
            return Err(Error::code(
                Code::E2001,
                format!("record exists at {}", root.display()),
            ));
        }
        if root.exists() {
            let mut rd = std::fs::read_dir(root).map_err(|e| Error::io(root, e))?;
            if rd.next().is_some() {
                return Err(Error::code(
                    Code::E2001,
                    format!("{} exists and is not empty", root.display()),
                ));
            }
        }
        for sub in ["main", "objects/sha256", "objects/tmp"] {
            let p = root.join(sub);
            std::fs::create_dir_all(&p).map_err(|e| Error::io(&p, e))?;
        }
        let header = RecordHeader {
            format: RECORD_FORMAT,
            project_id: project_id.to_owned(),
            owner_id: owner_id.to_owned(),
            created: created.to_owned(),
            hash_alg: "sha256".to_owned(),
        };
        write_small_json(&root.join("record.json"), &header.to_json())?;
        let record = Record {
            root: root.to_path_buf(),
            header,
            segment_limit: SEGMENT_LIMIT,
            read_only: false,
        };
        let head = Head::initial(record.genesis());
        head.write(&record.main_dir())?;
        fsync_dir(&record.main_dir())?;
        fsync_dir(root)?;
        Ok(record)
    }

    /// Open an existing record directory.
    ///
    /// # Errors
    /// `E2103` when `record.json` is missing or malformed; `Error::Io`.
    pub fn open(root: &Path) -> Result<Record> {
        let path = root.join("record.json");
        let v = read_small_json(&path).map_err(|e| match e {
            Error::Io { source, .. } if source.kind() == std::io::ErrorKind::NotFound => {
                Error::code(
                    Code::E2004,
                    format!(
                        "{} is not a record directory (no record.json)",
                        root.display()
                    ),
                )
            }
            other => other,
        })?;
        let header = RecordHeader::from_json(&v)
            .ok_or_else(|| Error::code(Code::E2103, "record.json is not a record header"))?;
        if header.hash_alg != "sha256" {
            return Err(Error::code(
                Code::E2103,
                format!("record.json names hash_alg {}, not sha256", header.hash_alg),
            ));
        }
        let read_only = header.format > RECORD_FORMAT;
        Ok(Record {
            root: root.to_path_buf(),
            header,
            segment_limit: SEGMENT_LIMIT,
            read_only,
        })
    }
}

/// The genesis string is also the initial `HEAD.last_id` (11.3).
#[must_use]
pub fn initial_last_id(project_id: &str) -> EventId {
    tm_formats::id::genesis("main", project_id)
}
