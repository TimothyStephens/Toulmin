//! Event bodies for exactly the gate A types (design 11.16, item 14).
//!
//! The type set is closed: [`EventType`] has one variant per 11.16 row and
//! nothing for the reserved namespaces, which read as opaque (11.11).
//! Each type's body schema is declared in its namespace's module as a list
//! of `(key, Form)`; a body of a known type and `tv` has exactly those keys
//! and every value of a stated form is checked. Every error is `E1002`
//! with `field` under `/body`.
//!
//! An [`Event`] is either a validated [`KnownEvent`] or an [`OpaqueEvent`];
//! only a `KnownEvent` can be written (see `envelope::write_line`).

mod audit;
mod chain;
mod claims;
mod files;
mod lock;
mod project;
mod reports;
mod settings;

use serde_json::{Map, Value};

use crate::assertion::Assertion;
use crate::citation::Citation;
use crate::errors::{Code, Error, ptr};
use crate::id::{
    ClaimRef, FileHandle, ReportHandle, VersionHandle, id_of, is_run_handle, is_sha256, positive,
};
use crate::parse_spec::ParseSpec;
use crate::steps::{OP_VERSION, Steps};
use crate::typed_value::{DateTime, TypedValue};
use crate::version;

/// The form a body value must have. `Any` is a value whose form 11.16 does
/// not state, accepted as any JSON value in gate A.
#[derive(Debug)]
pub(crate) enum Form {
    Any,
    Str,
    NonEmptyStr,
    Int,
    /// A non-negative integer written as a decimal string (`ino`, 11.16).
    IntStr,
    /// An integer of at least 1.
    IntFrom1,
    Bool,
    /// The 11.2 `at` form.
    At,
    /// The 11.9 `expected` shape: a typed value or `[lo, hi]`.
    Expected,
    /// An object from version handles to IDs (`versions.parse_specs`).
    SpecMap,
    File,
    Version,
    Claim,
    Report,
    Id,
    Choice(&'static [&'static str]),
    Typed,
    Snapshot,
    Manifest,
    Origin,
    Spec,
    Steps,
    Assertion,
    Citation,
    List(&'static Form),
    Null(&'static Form),
    Object(&'static [(&'static str, Form)]),
}

/// One gate A event type.
#[derive(Debug)]
pub(crate) struct TypeDef {
    pub ty: EventType,
    pub name: &'static str,
    pub tv: u64,
    pub keys: &'static [(&'static str, Form)],
}

/// The gate A event types (11.16).
#[allow(missing_docs, reason = "each variant is named after its 11.16 type")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventType {
    ProjectCreated,
    SettingsChanged,
    FileRegistered,
    FileVersionAdded,
    FileChecksumChecked,
    FileParseSpecSet,
    FileKept,
    FileMarkedRemovable,
    FileRestored,
    FileRenamed,
    FilePanelsSet,
    ReportRegistered,
    ClaimAdded,
    ClaimChecked,
    ClaimRevised,
    ClaimReviewed,
    ClaimRetracted,
    ClaimSuperseded,
    AuditCompleted,
    ChainTornTailTruncated,
    LockTakenOver,
}

const NAMESPACES: &[&[TypeDef]] = &[
    project::TYPES,
    settings::TYPES,
    files::TYPES,
    reports::TYPES,
    claims::TYPES,
    audit::TYPES,
    chain::TYPES,
    lock::TYPES,
];

fn defs() -> impl Iterator<Item = &'static TypeDef> {
    NAMESPACES.iter().flat_map(|n| n.iter())
}

/// The definition a writer uses: the type's highest `tv`.
fn def_of(ty: EventType) -> &'static TypeDef {
    defs()
        .filter(|d| d.ty == ty)
        .max_by_key(|d| d.tv)
        .unwrap_or(&project::TYPES[0])
}

/// The definition of one `(type, tv)`, if the reader knows it.
fn def_at(ty: EventType, tv: u64) -> Option<&'static TypeDef> {
    defs().find(|d| d.ty == ty && d.tv == tv)
}

impl EventType {
    /// Every gate A type, in 11.16 order, once each.
    #[must_use]
    pub fn all() -> Vec<EventType> {
        let mut out: Vec<EventType> = Vec::new();
        for d in defs() {
            if !out.contains(&d.ty) {
                out.push(d.ty);
            }
        }
        out
    }

    /// The type by its namespaced name, if it is a gate A type.
    #[must_use]
    pub fn from_name(name: &str) -> Option<EventType> {
        defs().find(|d| d.name == name).map(|d| d.ty)
    }

    /// The namespaced name, for example `claim.added`.
    #[must_use]
    pub fn name(self) -> &'static str {
        def_of(self).name
    }

    /// The type's current version (`tv`): the one a writer writes.
    #[must_use]
    pub fn tv(self) -> u64 {
        def_of(self).tv
    }

    /// Every version of the type a reader knows, ascending (11.11).
    #[must_use]
    pub fn known_tvs(self) -> Vec<u64> {
        let mut tvs: Vec<u64> = defs().filter(|d| d.ty == self).map(|d| d.tv).collect();
        tvs.sort_unstable();
        tvs
    }
}

/// A validated event of a gate A type at a known `tv`.
#[derive(Debug, Clone, PartialEq)]
pub struct KnownEvent {
    ty: EventType,
    tv: u64,
    body: Map<String, Value>,
}

impl KnownEvent {
    /// Validate a body for a type at its current `tv` (what a writer
    /// builds); errors point under `/body`.
    ///
    /// # Errors
    /// `E1002` with `field` at the offending member.
    pub fn new(ty: EventType, body: &Value) -> Result<KnownEvent, Error> {
        Self::new_at(ty, ty.tv(), body)
    }

    /// Validate a body for a type at a given `tv` (what a reader does with
    /// a stored line of an older version).
    ///
    /// # Errors
    /// `E1002` at `/tv` for a version the reader does not know, else with
    /// `field` at the offending member.
    pub fn new_at(ty: EventType, tv: u64, body: &Value) -> Result<KnownEvent, Error> {
        let Some(def) = def_at(ty, tv) else {
            return Err(Error::at(
                Code::E1002,
                "/tv",
                format!("unknown version {tv} of {}", ty.name()),
            ));
        };
        let Some(m) = body.as_object() else {
            return Err(Error::at(Code::E1002, "/body", "a body is an object"));
        };
        check_object(m, def.keys, "/body")?;
        if ty == EventType::FileParseSpecSet {
            let fp = ParseSpec::from_json(&m["spec"])
                .map_err(|x| x.in_envelope("/body/spec"))?
                .fingerprint();
            if m["spec_fingerprint"].as_str() != Some(fp.as_str()) {
                return Err(e(
                    "/body/spec_fingerprint",
                    "`spec_fingerprint` is the fingerprint of `spec`",
                ));
            }
        }
        Ok(KnownEvent {
            ty,
            tv,
            body: m.clone(),
        })
    }

    /// The type.
    #[must_use]
    pub fn ty(&self) -> EventType {
        self.ty
    }

    /// The version this body was validated at.
    #[must_use]
    pub fn tv(&self) -> u64 {
        self.tv
    }

    /// The body.
    #[must_use]
    pub fn body(&self) -> &Map<String, Value> {
        &self.body
    }

    /// One body member.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.body.get(key)
    }
}

/// An event this reader does not interpret: an unknown type, a `tv` above
/// the known one, or an envelope version above 1. Kept as given; it cannot
/// be written.
#[derive(Debug, Clone, PartialEq)]
pub struct OpaqueEvent {
    /// The `type` as given (empty when the envelope itself is opaque).
    pub ty: String,
    /// The `tv` as given (0 when the envelope itself is opaque).
    pub tv: u64,
    /// The whole envelope without `id`.
    pub envelope: Value,
}

/// A read event.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// Validated and interpreted.
    Known(KnownEvent),
    /// Kept as given, never interpreted (11.11).
    Opaque(OpaqueEvent),
}

fn e(at: &str, msg: impl Into<String>) -> Error {
    Error::at(Code::E1002, at, msg)
}

pub(crate) fn check_object(
    m: &Map<String, Value>,
    keys: &[(&str, Form)],
    at: &str,
) -> Result<(), Error> {
    for (k, _) in keys {
        if !m.contains_key(*k) {
            return Err(e(&ptr(at, k), format!("required member `{k}` is absent")));
        }
    }
    let mut extra: Vec<&String> = m
        .keys()
        .filter(|k| !keys.iter().any(|(n, _)| n == k))
        .collect();
    extra.sort_by(|a, b| crate::canonical::utf16_cmp(a, b));
    if let Some(k) = extra.first() {
        return Err(e(&ptr(at, k), format!("member `{k}` is not defined here")));
    }
    for (k, form) in keys {
        check(&m[*k], form, &ptr(at, k))?;
    }
    Ok(())
}

fn whole_second_utc(s: &str) -> bool {
    s.len() == 20 && DateTime::parse(s).is_some()
}

/// The envelope's `at` form, also used for snapshot times.
pub(crate) fn is_at(s: &str) -> bool {
    whole_second_utc(s)
}

pub(crate) fn check(v: &Value, form: &Form, at: &str) -> Result<(), Error> {
    let s = v.as_str();
    match form {
        Form::Any => Ok(()),
        Form::Str => s.map(|_| ()).ok_or_else(|| e(at, "a string")),
        Form::NonEmptyStr => s
            .filter(|s| !s.is_empty())
            .map(|_| ())
            .ok_or_else(|| e(at, "a non-empty string")),
        Form::Int => v
            .as_u64()
            .map(|_| ())
            .ok_or_else(|| e(at, "a non-negative integer")),
        Form::IntFrom1 => v
            .as_u64()
            .filter(|n| *n >= 1)
            .map(|_| ())
            .ok_or_else(|| e(at, "an integer of at least 1")),
        Form::Bool => v.as_bool().map(|_| ()).ok_or_else(|| e(at, "a boolean")),
        Form::At => s
            .filter(|s| is_at(s))
            .map(|_| ())
            .ok_or_else(|| e(at, "a time YYYY-MM-DDTHH:MM:SSZ")),
        Form::Expected => check_expected(v, at),
        Form::SpecMap => check_spec_map(v, at),
        Form::IntStr => s
            .filter(|s| *s == "0" || positive(s).is_some())
            .map(|_| ())
            .ok_or_else(|| e(at, "a non-negative integer as a decimal string")),
        Form::File => s
            .and_then(FileHandle::parse)
            .map(|_| ())
            .ok_or_else(|| e(at, "a file handle F<n>")),
        Form::Report => s
            .and_then(ReportHandle::parse)
            .map(|_| ())
            .ok_or_else(|| e(at, "a report handle T<n>")),
        Form::Version => s
            .and_then(VersionHandle::parse)
            .map(|_| ())
            .ok_or_else(|| e(at, "a version handle F<n>@v<m>")),
        Form::Claim => s
            .and_then(ClaimRef::parse)
            .map(|_| ())
            .ok_or_else(|| e(at, "a claim ref C<n>")),
        Form::Id => s
            .filter(|s| is_sha256(s))
            .map(|_| ())
            .ok_or_else(|| e(at, "sha256: and 64 hex digits")),
        Form::Choice(allowed) => match s {
            Some(s) if allowed.contains(&s) => Ok(()),
            _ => Err(e(at, format!("one of {allowed:?}"))),
        },
        Form::Typed => TypedValue::from_json(v, at)
            .map(|_| ())
            .map_err(|x| x.in_envelope("")),
        Form::Spec => ParseSpec::from_json(v)
            .map(|_| ())
            .map_err(|x| x.in_envelope(at)),
        Form::Steps => Steps::from_json(v)
            .map(|_| ())
            .map_err(|x| x.in_envelope(at)),
        Form::Assertion => Assertion::from_json(v)
            .map(|_| ())
            .map_err(|x| x.in_envelope(at)),
        Form::Citation => Citation::from_json(v, at)
            .map(|_| ())
            .map_err(|x| x.in_envelope("")),
        Form::List(inner) => {
            let a = v.as_array().ok_or_else(|| e(at, "a list"))?;
            for (i, item) in a.iter().enumerate() {
                check(item, inner, &ptr(at, i))?;
            }
            Ok(())
        }
        Form::Null(inner) => {
            if v.is_null() {
                Ok(())
            } else {
                check(v, inner, at)
            }
        }
        Form::Object(keys) => {
            let m = v.as_object().ok_or_else(|| e(at, "an object"))?;
            check_object(m, keys, at)
        }
        Form::Origin => check_origin(v, at),
        Form::Snapshot => check_snapshot(v, at),
        Form::Manifest => check_manifest(v, at),
    }
}

/// `file.version_added.origin` (11.16).
fn check_origin(v: &Value, at: &str) -> Result<(), Error> {
    let m = v.as_object().ok_or_else(|| e(at, "an object"))?;
    check_object(
        m,
        &[
            (
                "kind",
                Form::Choice(&["registered", "run", "changed_outside_run"]),
            ),
            ("run", Form::Any),
        ],
        at,
    )?;
    let run = &m["run"];
    if !(run.is_null() || run.as_str().is_some_and(is_run_handle)) {
        return Err(e(&ptr(at, "run"), "a run handle R<n> or null"));
    }
    Ok(())
}

/// The 11.9 `expected` shape: a typed value, or `[lo, hi]` of two.
fn check_expected(v: &Value, at: &str) -> Result<(), Error> {
    match v.as_array() {
        Some(pair) if pair.len() == 2 => {
            for (i, x) in pair.iter().enumerate() {
                check(x, &Form::Typed, &ptr(at, i))?;
            }
            Ok(())
        }
        Some(_) => Err(e(at, "`expected` is a typed value or [lo, hi]")),
        None => check(v, &Form::Typed, at),
    }
}

/// `versions.parse_specs`: version handles to spec fingerprint IDs.
fn check_spec_map(v: &Value, at: &str) -> Result<(), Error> {
    let m = v.as_object().ok_or_else(|| e(at, "an object"))?;
    for (k, x) in m {
        if VersionHandle::parse(k).is_none() {
            return Err(e(&ptr(at, k), "a version handle F<n>@v<m>"));
        }
        check(x, &Form::Id, &ptr(at, k))?;
    }
    Ok(())
}

/// A version above every known one (11.11). Only an integer counts: a
/// malformed version is a body error, not a later format.
fn above(v: &Value, known: u64) -> bool {
    v.as_u64().is_some_and(|n| n > known)
}

/// True when a nested format inside `body` carries a version above the
/// reader's highest known one (11.16): the event is then opaque, and the
/// body is not checked.
#[must_use]
pub fn nested_version_above(ty: EventType, body: &Value) -> bool {
    match ty {
        EventType::ClaimAdded | EventType::ClaimRevised => {
            let steps = &body["steps"];
            above(&steps["steps"], version::STEPS)
                || steps["ops"]
                    .as_array()
                    .is_some_and(|ops| ops.iter().any(|op| above(&op["v"], OP_VERSION)))
                || above(&body["assertion"]["assert"], version::ASSERTION)
        }
        EventType::FileParseSpecSet => above(&body["spec"]["spec"], version::PARSE_SPEC),
        _ => false,
    }
}

fn check_snapshot(v: &Value, at: &str) -> Result<(), Error> {
    let m = v
        .as_object()
        .ok_or_else(|| e(at, "a snapshot is an object"))?;
    check_object(
        m,
        &[
            ("size", Form::Int),
            ("mtime", Form::Str),
            ("mtime_ns", Form::Int),
            ("ctime", Form::Str),
            ("ctime_ns", Form::Int),
            ("ino", Form::IntStr),
        ],
        at,
    )?;
    for k in ["mtime", "ctime"] {
        if !m[k].as_str().is_some_and(is_at) {
            return Err(e(&ptr(at, k), "an RFC 3339 UTC time in whole seconds"));
        }
        let ns = format!("{k}_ns");
        if m[&ns].as_u64().is_none_or(|n| n > 999_999_999) {
            return Err(e(&ptr(at, &ns), "nanoseconds, 0 to 999999999"));
        }
    }
    Ok(())
}

static ENTRY: [(&str, Form); 3] = [
    ("path", Form::NonEmptyStr),
    ("snapshot", Form::Snapshot),
    ("fingerprint", Form::Id),
];
static ENTRIES: Form = Form::List(&Form::Object(&ENTRY));
static MANIFEST: [(&str, Form); 3] = [
    ("fingerprint", Form::Id),
    ("ignore", Form::List(&Form::Str)),
    ("entries", Form::Any),
];

fn check_manifest(v: &Value, at: &str) -> Result<(), Error> {
    let m = v
        .as_object()
        .ok_or_else(|| e(at, "a manifest is an object"))?;
    check_object(m, &MANIFEST, at)?;
    check(&m["entries"], &ENTRIES, &ptr(at, "entries"))?;
    let entries = m["entries"].as_array().map_or(&[][..], Vec::as_slice);
    for i in 1..entries.len() {
        let a = entries[i - 1]["path"].as_str().unwrap_or_default();
        let b = entries[i]["path"].as_str().unwrap_or_default();
        if crate::sort_order::cmp_text(a, b).is_ge() {
            return Err(e(
                &ptr(&ptr(&ptr(at, "entries"), i), "path"),
                "entries are sorted by path in 11.10 order, without repeats",
            ));
        }
    }
    // 11.16 (decision 10 as amended 2026-11-11): the fingerprint digests
    // `[{"path","fingerprint"}]` in entry order; snapshots stay outside it.
    let pairs: Vec<Value> = entries
        .iter()
        .map(|e| serde_json::json!({"path": e["path"], "fingerprint": e["fingerprint"]}))
        .collect();
    let fp = id_of(&Value::Array(pairs)).map_err(|x| x.in_envelope(at))?;
    if m["fingerprint"].as_str() != Some(fp.as_str()) {
        return Err(e(
            &ptr(at, "fingerprint"),
            "the fingerprint is the ID of the entries' [{path, fingerprint}] list",
        ));
    }
    Ok(())
}
