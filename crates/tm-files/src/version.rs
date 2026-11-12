//! File versions (item 45; 6.2; D5, D22): a version is one fingerprint
//! with its kept state, origin, and content label. A candidate (what
//! `tm-fsfacts` found at a path now) becomes a new input version only when
//! it differs from the latest version under all three equivalences: same
//! bytes, same table by key, same decompressed content (`.gz` and `.bgz`,
//! the latter concatenated gzip members, read with a multi-member decoder).

use std::collections::BTreeMap;
use std::path::Path;

use arrow::array::RecordBatch;
use flate2::read::MultiGzDecoder;
use serde_json::{Value, json};
use tm_formats::events::{EventType, KnownEvent};
use tm_formats::id::{EventId, FileHandle, VersionHandle};
use tm_formats::parse_spec::ParseSpec;
use tm_formats::typed_value::TypedValue;
use tm_fsfacts::hash::{HashSettings, hash_file, hash_reader};
use tm_fsfacts::manifest::Manifest;
use tm_fsfacts::snapshot::Snapshot;

use crate::attributes::{Action, Kept, State};
use crate::parse::{key_indexes, key_tuple_json, typed_rows};
use crate::{Error, Result, event};

/// 6.2 Origin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// `tm file register` or `tm file hash`.
    Registered,
    /// A run (gate C), by handle.
    Run(String),
    /// A hand edit found by a disk scan.
    ChangedOutsideRun,
}

impl Origin {
    /// The 11.16 `origin` object.
    #[must_use]
    pub fn to_json(&self) -> Value {
        match self {
            Origin::Registered => json!({"kind": "registered", "run": null}),
            Origin::Run(r) => json!({"kind": "run", "run": r}),
            Origin::ChangedOutsideRun => json!({"kind": "changed_outside_run", "run": null}),
        }
    }

    /// The `origin.kind` spelling.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Origin::Registered => "registered",
            Origin::Run(_) => "run",
            Origin::ChangedOutsideRun => "changed_outside_run",
        }
    }
}

/// 6.2 Content label, spelled as 11.16 does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentLabel {
    /// New content.
    ContentChanged,
    /// Not rewritten by the run; content matches an earlier version (a run
    /// output's label, gate C).
    CarriedForward,
    /// Snapshot changed, content did not (a run output's label, gate C).
    MetadataOnly,
}

impl ContentLabel {
    /// The 11.16 spelling.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            ContentLabel::ContentChanged => "content_changed",
            ContentLabel::CarriedForward => "carried_forward",
            ContentLabel::MetadataOnly => "metadata_only",
        }
    }
}

/// What `tm-fsfacts` found at a path: the facts a version would record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// `sha256:` of the bytes, or the manifest fingerprint of a directory.
    pub fingerprint: EventId,
    /// Bytes, or a directory's entry sizes summed (item 28).
    pub size: u64,
    /// The stat facts.
    pub snapshot: Snapshot,
    /// The 11.16 `manifest` object for a directory, `None` for a file.
    pub manifest: Option<Value>,
    /// `sha256:` of the decompressed bytes, for a `.gz` or `.bgz` file.
    pub decompressed: Option<EventId>,
}

impl Candidate {
    /// Hash and snapshot a regular file (a symbolic link is followed).
    ///
    /// # Errors
    /// `Error::Io`.
    pub fn of_file(path: &Path, settings: &HashSettings) -> Result<Candidate> {
        let hashed = hash_file(path, settings)?;
        let decompressed = if is_compressed_name(path) {
            Some(decompressed_fingerprint(path, settings)?)
        } else {
            None
        };
        Ok(Candidate {
            fingerprint: hashed.fingerprint,
            size: hashed.bytes,
            snapshot: Snapshot::take(path)?,
            manifest: None,
            decompressed,
        })
    }

    /// Walk, hash, and snapshot a directory (item 28).
    ///
    /// # Errors
    /// `E3003` for a non-UTF-8 entry; `Error::Io`.
    pub fn of_directory(
        path: &Path,
        ignore: &[String],
        settings: &HashSettings,
    ) -> Result<Candidate> {
        let manifest = Manifest::build(path, ignore, settings)?;
        Ok(Candidate {
            fingerprint: manifest.fingerprint()?,
            size: manifest.size(),
            snapshot: Snapshot::take(path)?,
            manifest: Some(manifest.to_json()?),
            decompressed: None,
        })
    }
}

/// One version of a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    /// `F<n>@v<m>`.
    pub handle: VersionHandle,
    /// The fingerprint.
    pub fingerprint: EventId,
    /// Bytes.
    pub size: u64,
    /// Stat facts when the version was recorded.
    pub snapshot: Snapshot,
    /// Where it came from.
    pub origin: Origin,
    /// How its content relates to the previous version; `None` for a first
    /// version.
    pub content_label: Option<ContentLabel>,
    /// The directory manifest, for a directory version.
    pub manifest: Option<Value>,
    /// `sha256:` of the decompressed bytes, for a `.gz` or `.bgz` version
    /// (not in the event body; the Core keeps it in the stat cache, 11.17).
    pub decompressed: Option<EventId>,
    /// Kept state.
    pub kept: Kept,
}

/// How a candidate relates to the latest version (D22).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Equivalence {
    /// Equal fingerprints.
    SameBytes,
    /// Both parse under the identity's spec to the same rows by key.
    SameTableByKey,
    /// `.gz` or `.bgz` files whose decompressed bytes are equal.
    SameDecompressed,
    /// None of the three.
    Different,
}

impl Equivalence {
    /// True for any of the three equivalences.
    #[must_use]
    pub fn is_same(self) -> bool {
        !matches!(self, Equivalence::Different)
    }
}

/// True for a path whose name ends in `.gz` or `.bgz`.
#[must_use]
pub fn is_compressed_name(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e == "gz" || e == "bgz")
}

/// The fingerprint of a `.gz` or `.bgz` file's decompressed bytes, which
/// the third equivalence compares: every gzip member is read in turn
/// (`.bgz` is concatenated members), through the hashing settings.
///
/// # Errors
/// `Error::Io`, including a stream that is not gzip.
pub fn decompressed_fingerprint(path: &Path, settings: &HashSettings) -> Result<EventId> {
    let file = std::fs::File::open(path).map_err(|e| Error::io(path, e))?;
    let decoder = MultiGzDecoder::new(file);
    Ok(hash_reader(decoder, path, settings)?.fingerprint)
}

fn values_equal(a: &TypedValue, b: &TypedValue) -> bool {
    match (a, b) {
        (TypedValue::Missing, TypedValue::Missing) | (TypedValue::Nan, TypedValue::Nan) => true,
        (TypedValue::Inf(x), TypedValue::Inf(y)) | (TypedValue::Bool(x), TypedValue::Bool(y)) => {
            x == y
        }
        _ => a.compare(b) == Some(std::cmp::Ordering::Equal),
    }
}

/// The second equivalence: two tables parsed under one spec hold the same
/// rows by key, every cell compared as a typed value (so `1.10` and `1.1`
/// agree, row order does not matter, and `missing` equals `missing`).
///
/// # Errors
/// `E3102` if either batch is not of the spec's item 47 types.
pub fn same_table_by_key(
    previous: &RecordBatch,
    candidate: &RecordBatch,
    spec: &ParseSpec,
) -> Result<bool> {
    if previous.num_rows() != candidate.num_rows() {
        return Ok(false);
    }
    let keys = key_indexes(spec);
    let index = |batch: &RecordBatch| -> Result<BTreeMap<String, Vec<TypedValue>>> {
        Ok(typed_rows(batch, spec)?
            .into_iter()
            .map(|row| (key_tuple_json(&row, &keys), row))
            .collect())
    };
    let a = index(previous)?;
    let b = index(candidate)?;
    if a.len() != b.len() {
        return Ok(false);
    }
    Ok(a.iter().all(|(k, row)| {
        b.get(k).is_some_and(|other| {
            row.len() == other.len() && row.iter().zip(other).all(|(x, y)| values_equal(x, y))
        })
    }))
}

/// Decide how `candidate` relates to `previous`. `tables` carries the
/// previous version's parsed table (from the parse cache) and the
/// candidate's, with the identity's spec, when the identity has one; the
/// decompressed fingerprints are the two values' own.
///
/// # Errors
/// As [`same_table_by_key`].
pub fn equivalence(
    previous: &Version,
    candidate: &Candidate,
    tables: Option<(&RecordBatch, &RecordBatch, &ParseSpec)>,
) -> Result<Equivalence> {
    if previous.fingerprint == candidate.fingerprint {
        return Ok(Equivalence::SameBytes);
    }
    if let Some((p, c, spec)) = tables
        && same_table_by_key(p, c, spec)?
    {
        return Ok(Equivalence::SameTableByKey);
    }
    if let (Some(a), Some(b)) = (&previous.decompressed, &candidate.decompressed)
        && a == b
    {
        return Ok(Equivalence::SameDecompressed);
    }
    Ok(Equivalence::Different)
}

/// Why a version was kept (11.16 `file.kept.reason`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeepReason {
    /// First promotion citing it (D4).
    Promotion,
    /// `tm file keep`.
    Owner,
}

impl Version {
    /// The next version of `file` for `candidate`, or `None` when the
    /// candidate is equivalent to `previous` (item 45): a first version has
    /// no content label; a later one is `content_changed`.
    ///
    /// # Errors
    /// `E1002` when the body does not validate (a directory manifest whose
    /// fingerprint `tm-formats` computes otherwise, see the P3 report).
    pub fn next(
        file: FileHandle,
        previous: Option<&Version>,
        candidate: &Candidate,
        origin: Origin,
        equivalent: bool,
    ) -> Result<Option<(Version, KnownEvent)>> {
        if previous.is_some() && equivalent {
            return Ok(None);
        }
        let n = previous.map_or(1, |p| p.handle.version + 1);
        let handle = VersionHandle { file, version: n };
        let content_label = previous.map(|_| ContentLabel::ContentChanged);
        let ev = event(
            EventType::FileVersionAdded,
            &json!({
                "file": file.to_string(),
                "version": handle.to_string(),
                "fingerprint": candidate.fingerprint.as_str(),
                "origin": origin.to_json(),
                "size": candidate.size,
                "snapshot": candidate.snapshot.to_json(),
                "content_label": content_label.map(ContentLabel::as_str),
                "manifest": candidate.manifest,
            }),
        )?;
        Ok(Some((
            Version {
                handle,
                fingerprint: candidate.fingerprint.clone(),
                size: candidate.size,
                snapshot: candidate.snapshot,
                origin,
                content_label,
                manifest: candidate.manifest.clone(),
                decompressed: candidate.decompressed.clone(),
                kept: Kept::No,
            },
            ev,
        )))
    }

    /// Record that the Record kept this version's bytes as `object`
    /// (`file.kept`); `state` is the identity's 6.3 state with this version
    /// as its latest.
    ///
    /// # Errors
    /// `E3201` when 6.3 does not allow keeping (not registered, already
    /// kept).
    pub fn kept(
        &self,
        state: State,
        object: &EventId,
        compressed_size: u64,
        reason: KeepReason,
        fingerprint_only: bool,
    ) -> Result<(Version, KnownEvent)> {
        let next = state.apply(Action::Keep { fingerprint_only })?;
        let ev = event(
            EventType::FileKept,
            &json!({
                "version": self.handle.to_string(),
                "object": object.as_str(),
                "compressed_size": compressed_size,
                "reason": match reason { KeepReason::Promotion => "promotion", KeepReason::Owner => "owner" },
                "fingerprint_only": fingerprint_only,
            }),
        )?;
        let mut v = self.clone();
        v.kept = next.latest.unwrap_or(Kept::Bytes);
        Ok((v, ev))
    }

    /// Rebuild a version from a `file.version_added` body (what the fold
    /// and callers reading the chain do).
    ///
    /// # Errors
    /// `E1002` when `body` is not such a body.
    pub fn from_added(body: &Value) -> Result<Version> {
        let ev = event(EventType::FileVersionAdded, body)?;
        let s = |k: &str| {
            ev.get(k)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        let bad =
            |what: &str| Error::code(crate::Code::E1002, format!("file.version_added: {what}"));
        let handle = VersionHandle::parse(&s("version")).ok_or_else(|| bad("version handle"))?;
        let fingerprint = EventId::parse(&s("fingerprint")).ok_or_else(|| bad("fingerprint"))?;
        let snapshot = ev
            .get("snapshot")
            .and_then(Snapshot::from_json)
            .ok_or_else(|| bad("snapshot"))?;
        let origin = match ev
            .get("origin")
            .and_then(|o| o.get("kind"))
            .and_then(Value::as_str)
        {
            Some("run") => Origin::Run(
                ev.get("origin")
                    .and_then(|o| o.get("run"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
            ),
            Some("changed_outside_run") => Origin::ChangedOutsideRun,
            _ => Origin::Registered,
        };
        let content_label = match ev.get("content_label").and_then(Value::as_str) {
            Some("content_changed") => Some(ContentLabel::ContentChanged),
            Some("carried_forward") => Some(ContentLabel::CarriedForward),
            Some("metadata_only") => Some(ContentLabel::MetadataOnly),
            _ => None,
        };
        Ok(Version {
            handle,
            fingerprint,
            size: ev.get("size").and_then(Value::as_u64).unwrap_or(0),
            snapshot,
            origin,
            content_label,
            manifest: ev.get("manifest").filter(|m| !m.is_null()).cloned(),
            decompressed: None,
            kept: Kept::No,
        })
    }
}
