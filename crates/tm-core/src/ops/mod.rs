//! The Core operations (section 9; item 74), one file per gate A row, and
//! the per-command context they share.
//!
//! [`Core`] is one command's view of a project: the open record, the index
//! brought to the chain's head, the resolved settings, and the detected
//! `src` facts. A locked command takes the lock through [`Core::lock`],
//! which runs read-in (gate A: the lock's own queued events), and commits
//! its events through [`Locked::commit`], which appends one batch and folds
//! it into the index. Every owner operation takes an `&OwnerCap` (rule 4).

pub mod apply;
pub mod audit;
pub mod claim_add;
pub mod claim_check;
pub mod claim_retract;
pub mod claim_review;
pub mod claim_supersede;
pub mod claim_update_text;
pub mod doctor;
pub mod file_attrs;
pub mod file_hash;
pub mod file_panels;
pub mod file_register;
pub mod lock;
pub mod project_init;
pub mod read_in;
pub mod report_add;
pub mod report_check;
pub mod settings_set;
pub mod status;
pub mod table_spec;

use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

use arrow::record_batch::RecordBatch;
use serde_json::Value;
use tm_files::parse_cache::ParseCache;
use tm_files::{Identity, Version};
use tm_formats::envelope::SourceKind;
use tm_formats::events::{Event, KnownEvent};
use tm_formats::id::{EventId, FileHandle, ReportHandle, VersionHandle};
use tm_formats::parse_spec::ParseSpec;
use tm_index::{Index, IndexTx, Param};
use tm_owner_cap::OwnerCap;
use tm_record::{Lock, LockHolder, Record};

use crate::actor::{Actor, Source};
use crate::batch::{self, Committed};
use crate::project::{Dirs, Project};
use crate::settings::Settings;
use crate::{Code, Error, Result, index};

/// One command's context.
pub struct Core {
    /// The project.
    pub project: Project,
    /// The index, at the chain's head when the command started.
    pub index: Index,
    /// The settings (11.19).
    pub settings: Settings,
    /// The D23 facts of this command.
    pub source: Source,
    /// The command's time, one `at` per batch (11.2).
    pub at: String,
    /// The working directory paths are given relative to, when not the
    /// repository root.
    pub cwd: Option<PathBuf>,
}

impl Core {
    /// Open the project at `repo_root`, bring the index to the chain's
    /// head, load the settings, and detect the command's `src` facts.
    ///
    /// # Errors
    /// Item 73's codes; `Error::Index`; `Error::Record`.
    pub fn open(repo_root: &Path, dirs: Dirs, source: SourceKind) -> Result<Core> {
        let project = Project::open(repo_root, dirs)?;
        Core::with_project(project, source)
    }

    /// As [`Core::open`] for an already resolved project.
    ///
    /// # Errors
    /// `Error::Index`; `Error::Record`.
    pub fn with_project(project: Project, source: SourceKind) -> Result<Core> {
        let mut idx = index::open(&project.index_path())?;
        index::sync(&mut idx, &project.record)?;
        let settings = {
            let mut tx = idx.begin()?;
            let s = Settings::load(&mut tx)?;
            tx.rollback()?;
            s
        };
        let src = Source::detect(source, &settings.harness_env_vars());
        Ok(Core {
            project,
            index: idx,
            settings,
            source: src,
            at: crate::now_at(),
            cwd: None,
        })
    }

    /// Use a detected source other than the process's (tests, `tm apply`).
    #[must_use]
    pub fn with_source(mut self, source: Source) -> Core {
        self.source = source;
        self
    }

    /// The record.
    #[must_use]
    pub fn record(&self) -> &Record {
        &self.project.record
    }

    /// A read transaction over the index.
    ///
    /// # Errors
    /// `Error::Index`.
    pub fn tx(&mut self) -> Result<IndexTx<'_>> {
        Ok(self.index.begin()?)
    }

    /// Re-read the settings after a `settings.changed` was folded.
    ///
    /// # Errors
    /// `Error::Index`.
    pub fn reload_settings(&mut self) -> Result<()> {
        let mut tx = self.index.begin()?;
        self.settings = Settings::load(&mut tx)?;
        tx.rollback()?;
        Ok(())
    }

    /// The parse cache beside the index (item 48; item 76).
    #[must_use]
    pub fn parse_cache(&self) -> ParseCache {
        self.env().parse_cache()
    }

    /// The engine settings (11.19), the parse cache directory as the
    /// default temp directory.
    #[must_use]
    pub fn engine_settings(&self) -> tm_duckdb::EngineSettings {
        self.env().engine_settings()
    }

    /// The project and settings without the index: what the byte and
    /// table helpers need, usable while a transaction borrows the index.
    #[must_use]
    pub fn env(&self) -> Env<'_> {
        Env {
            project: &self.project,
            settings: &self.settings,
        }
    }

    /// The repository-relative form of a path the command was given.
    ///
    /// # Errors
    /// `E3001`.
    pub fn relative(&self, path: &Path) -> Result<String> {
        self.project.relative(path, self.cwd.as_deref())
    }

    /// Take the main-chain lock (3.1) with the settings' heartbeat and
    /// takeover, waiting for a live holder (`on_wait` is told who holds it
    /// and the time until a takeover is allowed), and run read-in.
    ///
    /// # Errors
    /// `E2201`, `E2103`, `E2008`, `Error::Io` from the Record.
    pub fn lock(&mut self, on_wait: impl FnMut(&LockHolder, Duration)) -> Result<Locked<'_>> {
        let opts = self.settings.lock_options();
        let lock = self.project.record.lock(&opts, on_wait)?;
        // Another writer may have appended before the lock was ours.
        index::sync(&mut self.index, &self.project.record)?;
        let queued = read_in::read_in(self, &lock)?;
        Ok(Locked {
            core: self,
            lock,
            read_in: queued,
        })
    }
}

/// The project and settings of a command, apart from its index, so a
/// helper can read bytes and tables while a transaction holds the index.
#[derive(Clone, Copy)]
pub struct Env<'a> {
    /// The project.
    pub project: &'a Project,
    /// The settings.
    pub settings: &'a Settings,
}

impl Env<'_> {
    /// The parse cache beside the index (item 48; item 76).
    #[must_use]
    pub fn parse_cache(&self) -> ParseCache {
        ParseCache::new(self.project.parse_cache_dir())
    }

    /// The engine settings (11.19), the parse cache directory as the
    /// default temp directory.
    #[must_use]
    pub fn engine_settings(&self) -> tm_duckdb::EngineSettings {
        let dir = self.project.parse_cache_dir();
        let _ = std::fs::create_dir_all(&dir);
        self.settings.engine_settings(&dir)
    }
}

/// Split a command into its environment and its index, so a transaction
/// over the index and the byte helpers can be used together.
#[must_use]
pub fn split(core: &mut Core) -> (Env<'_>, &mut Index) {
    (
        Env {
            project: &core.project,
            settings: &core.settings,
        },
        &mut core.index,
    )
}

/// A command holding the lock.
pub struct Locked<'a> {
    /// The context.
    pub core: &'a mut Core,
    /// The lock.
    pub lock: Lock,
    /// What read-in produced (gate A: nothing beyond the lock's queue).
    pub read_in: read_in::ReadIn,
}

impl Locked<'_> {
    /// Append `events` as one batch (the lock's queued events first) and
    /// fold it into the index. With nothing to write (no events, nothing
    /// queued) nothing is appended.
    ///
    /// # Errors
    /// The Record's append errors; `Error::Index`.
    pub fn commit(&mut self, actor: &Actor<'_>, events: &[KnownEvent]) -> Result<Committed> {
        let record = &self.core.project.record;
        if events.is_empty() && batch::queued(&self.lock)?.is_empty() {
            return Ok(Committed {
                ids: Vec::new(),
                queued: 0,
                head: record.head()?,
            });
        }
        let owner_id = record.header().owner_id.clone();
        let committed = batch::commit(
            record,
            actor.cap(),
            &self.lock,
            &self.core.at,
            &actor.envelope_actor(&owner_id),
            &self.core.source.envelope_source(),
            events,
        )?;
        index::sync(&mut self.core.index, &self.core.project.record)?;
        Ok(committed)
    }

    /// The IDs [`Locked::commit`] would give `events` now (item 75): a
    /// `claim.checked` names its revision's ID before both are written.
    ///
    /// # Errors
    /// `E1002` from the envelope writer.
    pub fn preview_ids(&self, actor: &Actor<'_>, events: &[KnownEvent]) -> Result<Vec<EventId>> {
        let record = &self.core.project.record;
        let owner_id = record.header().owner_id.clone();
        batch::preview_ids(
            record,
            &self.lock,
            &self.core.at,
            &actor.envelope_actor(&owner_id),
            &self.core.source.envelope_source(),
            events,
        )
    }

    /// Release the lock.
    ///
    /// # Errors
    /// `Error::Io`.
    pub fn release(self) -> Result<()> {
        self.lock.release()?;
        Ok(())
    }
}

// ----------------------------------------------------------- index readers

/// A `files` row (11.17).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRow {
    /// The identity.
    pub identity: Identity,
    /// `files.restored`.
    pub restored: bool,
    /// The `file.registered` event, if registered.
    pub registered_event: Option<String>,
}

/// A `file_versions` row (11.17).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionRow {
    /// `F<n>@v<m>`.
    pub handle: VersionHandle,
    /// The fingerprint.
    pub fingerprint: EventId,
    /// `origin_kind`.
    pub origin_kind: String,
    /// `origin_run`.
    pub origin_run: Option<String>,
    /// `content_label`.
    pub content_label: Option<String>,
    /// `size`.
    pub size: u64,
    /// `kept_object`.
    pub kept_object: Option<String>,
    /// `fingerprint_only`.
    pub fingerprint_only: bool,
    /// `decompressed_fingerprint`.
    pub decompressed_fingerprint: Option<String>,
    /// The `file.version_added` event.
    pub event_id: String,
}

impl VersionRow {
    /// Bytes are kept (`file.kept` without `fingerprint_only`).
    #[must_use]
    pub fn bytes_kept(&self) -> bool {
        self.kept_object.is_some() && !self.fingerprint_only
    }

    /// The 6.2 kept state.
    #[must_use]
    pub fn kept(&self) -> tm_files::Kept {
        match (&self.kept_object, self.fingerprint_only) {
            (None, _) => tm_files::Kept::No,
            (Some(_), true) => tm_files::Kept::FingerprintOnly,
            (Some(_), false) => tm_files::Kept::Bytes,
        }
    }
}

const FILE_COLUMNS: &str = "file, kind, path, title, description, source, level, retired, restored, removable, agent_denied, registered_event";

fn file_row(r: &tm_index::Row) -> Option<FileRow> {
    let text = |i: usize| r.text(i).map(str::to_owned);
    let flag = |i: usize| r.int(i).unwrap_or(0) != 0;
    Some(FileRow {
        identity: Identity {
            file: FileHandle::parse(r.text(0)?)?,
            kind: match r.text(1)? {
                "input" => tm_files::Kind::Input,
                _ => return None,
            },
            path: text(2)?,
            title: text(3),
            description: text(4),
            source: text(5),
            level: match r.text(6)? {
                "registered" => tm_files::Level::Registered,
                _ => tm_files::Level::Tracked,
            },
            retired: flag(7),
            removable: flag(9),
            agent_denied: flag(10),
        },
        restored: flag(8),
        registered_event: text(11),
    })
}

/// Every `files` row, by handle number.
///
/// # Errors
/// `Error::Index`.
pub fn files(tx: &mut IndexTx<'_>) -> Result<Vec<FileRow>> {
    let rows = tx.query_rows(
        &format!("SELECT {FILE_COLUMNS} FROM files ORDER BY CAST(substr(file, 2) AS INTEGER)"),
        &[],
    )?;
    Ok(rows.iter().filter_map(file_row).collect())
}

/// The `files` row for a handle.
///
/// # Errors
/// `Error::Index`.
pub fn file_by_handle(tx: &mut IndexTx<'_>, file: FileHandle) -> Result<Option<FileRow>> {
    let rows = tx.query_rows(
        &format!("SELECT {FILE_COLUMNS} FROM files WHERE file = ?"),
        &[Param::from(file.to_string())],
    )?;
    Ok(rows.first().and_then(file_row))
}

/// The `files` row for a repository-relative path.
///
/// # Errors
/// `Error::Index`.
pub fn file_by_path(tx: &mut IndexTx<'_>, path: &str) -> Result<Option<FileRow>> {
    let rows = tx.query_rows(
        &format!("SELECT {FILE_COLUMNS} FROM files WHERE path = ?"),
        &[Param::from(path)],
    )?;
    Ok(rows.first().and_then(file_row))
}

/// Resolve `F<n>` or a path to its `files` row.
///
/// # Errors
/// `E6001` when neither names a registered file; `Error::Index`.
pub fn file_by_handle_or_path(core: &mut Core, text: &str) -> Result<FileRow> {
    let rel = match FileHandle::parse(text) {
        Some(_) => None,
        None => core.relative(Path::new(text)).ok(),
    };
    let mut tx = core.tx()?;
    let row = match FileHandle::parse(text) {
        Some(h) => file_by_handle(&mut tx, h)?,
        None => match rel {
            Some(r) => file_by_path(&mut tx, &r)?,
            None => file_by_path(&mut tx, text)?,
        },
    };
    tx.rollback()?;
    row.ok_or_else(|| Error::code(Code::E6001, format!("{text} is not a registered file")))
}

const VERSION_COLUMNS: &str = "version, fingerprint, origin_kind, origin_run, content_label, size, kept_object, fingerprint_only, decompressed_fingerprint, event_id";

fn version_row(r: &tm_index::Row) -> Option<VersionRow> {
    let text = |i: usize| r.text(i).map(str::to_owned);
    Some(VersionRow {
        handle: VersionHandle::parse(r.text(0)?)?,
        fingerprint: EventId::parse(r.text(1)?)?,
        origin_kind: text(2)?,
        origin_run: text(3),
        content_label: text(4),
        size: r.get(5).as_u64().unwrap_or(0),
        kept_object: text(6),
        fingerprint_only: r.int(7).unwrap_or(0) != 0,
        decompressed_fingerprint: text(8),
        event_id: text(9)?,
    })
}

/// The versions of a file, by number.
///
/// # Errors
/// `Error::Index`.
pub fn versions_of(tx: &mut IndexTx<'_>, file: FileHandle) -> Result<Vec<VersionRow>> {
    let rows = tx.query_rows(
        &format!("SELECT {VERSION_COLUMNS} FROM file_versions WHERE file = ? ORDER BY n"),
        &[Param::from(file.to_string())],
    )?;
    Ok(rows.iter().filter_map(version_row).collect())
}

/// One version's row.
///
/// # Errors
/// `Error::Index`.
pub fn version_by_handle(
    tx: &mut IndexTx<'_>,
    version: &VersionHandle,
) -> Result<Option<VersionRow>> {
    let rows = tx.query_rows(
        &format!("SELECT {VERSION_COLUMNS} FROM file_versions WHERE version = ?"),
        &[Param::from(version.to_string())],
    )?;
    Ok(rows.first().and_then(version_row))
}

/// The latest version of a file.
///
/// # Errors
/// `Error::Index`.
pub fn latest_version(tx: &mut IndexTx<'_>, file: FileHandle) -> Result<Option<VersionRow>> {
    Ok(versions_of(tx, file)?.pop())
}

/// A file's parse spec and its fingerprint.
///
/// # Errors
/// `Error::Index`; `E3105` for a stored spec that does not read (never
/// from a chain the writer validated).
pub fn spec_of(tx: &mut IndexTx<'_>, file: FileHandle) -> Result<Option<(ParseSpec, EventId)>> {
    let rows = tx.query_rows(
        "SELECT spec, spec_fingerprint FROM parse_specs WHERE file = ?",
        &[Param::from(file.to_string())],
    )?;
    let Some(r) = rows.first() else {
        return Ok(None);
    };
    let v: Value = serde_json::from_str(r.text(0).unwrap_or("null"))
        .map_err(|e| Error::code(Code::E3105, format!("stored spec: {e}")))?;
    let spec = ParseSpec::from_json(&v)?;
    let fp = EventId::parse(r.text(1).unwrap_or_default())
        .ok_or_else(|| Error::code(Code::E3105, "stored spec fingerprint"))?;
    Ok(Some((spec, fp)))
}

/// The panel names registered on a version.
///
/// # Errors
/// `Error::Index`.
pub fn panels_of(tx: &mut IndexTx<'_>, version: &VersionHandle) -> Result<Vec<String>> {
    let rows = tx.query_rows(
        "SELECT name FROM panels WHERE version = ? ORDER BY name",
        &[Param::from(version.to_string())],
    )?;
    Ok(rows
        .iter()
        .filter_map(|r| r.text(0).map(str::to_owned))
        .collect())
}

/// The next `F<n>`: one above the highest registered (11.16).
///
/// # Errors
/// `Error::Index`.
pub fn next_file_handle(tx: &mut IndexTx<'_>) -> Result<FileHandle> {
    let rows = tx.query_rows("SELECT file FROM files", &[])?;
    let max = rows
        .iter()
        .filter_map(|r| r.text(0).and_then(FileHandle::parse))
        .map(|h| h.0)
        .max()
        .unwrap_or(0);
    Ok(FileHandle(max + 1))
}

/// The next `T<n>` (11.16: the next number after the highest registered
/// report).
///
/// # Errors
/// `Error::Index`.
pub fn next_report_handle(tx: &mut IndexTx<'_>) -> Result<ReportHandle> {
    let rows = tx.query_rows("SELECT report FROM reports", &[])?;
    let max = rows
        .iter()
        .filter_map(|r| r.text(0).and_then(ReportHandle::parse))
        .map(|h| h.0)
        .max()
        .unwrap_or(0);
    Ok(ReportHandle(max + 1))
}

/// A `reports` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportRow {
    /// `T<n>`.
    pub report: ReportHandle,
    /// The repository-relative path.
    pub path: String,
    /// The `report.registered` event.
    pub event_id: String,
}

/// Every registered report, by number.
///
/// # Errors
/// `Error::Index`.
pub fn reports(tx: &mut IndexTx<'_>) -> Result<Vec<ReportRow>> {
    let rows = tx.query_rows(
        "SELECT report, path, event_id FROM reports ORDER BY CAST(substr(report, 2) AS INTEGER)",
        &[],
    )?;
    Ok(rows
        .iter()
        .filter_map(|r| {
            Some(ReportRow {
                report: ReportHandle::parse(r.text(0)?)?,
                path: r.text(1)?.to_owned(),
                event_id: r.text(2)?.to_owned(),
            })
        })
        .collect())
}

/// Resolve a `T<n>` or a repository-relative path to its report (section
/// 9's `claim.add` row, item 74).
///
/// # Errors
/// `E6201` when none matches; `Error::Index`.
pub fn report_by_handle_or_path(core: &mut Core, text: &str) -> Result<ReportRow> {
    let rel = core.relative(Path::new(text)).ok();
    let mut tx = core.tx()?;
    let all = reports(&mut tx)?;
    tx.rollback()?;
    let handle = ReportHandle::parse(text);
    all.into_iter()
        .find(|r| {
            handle.is_some_and(|h| h == r.report)
                || rel.as_deref() == Some(r.path.as_str())
                || r.path == text
        })
        .ok_or_else(|| {
            Error::code(
                Code::E6201,
                format!("{text} is not a registered report (`tm report add`)"),
            )
        })
}

// ------------------------------------------------------- version contents

/// Rebuild a version from its `file.version_added` event, resolved by ID
/// through the index (section 9's resolve-by-ID rule).
///
/// # Errors
/// `E2103` when the event is not on the chain; `E1002` for a body that
/// does not read; `Error::Index`.
pub fn read_version(core: &mut Core, row: &VersionRow) -> Result<Version> {
    let mut tx = core.index.begin()?;
    let stored = index::read_event(&mut tx, &core.project.record, &row.event_id)?;
    tx.rollback()?;
    let body = match &stored.line.event {
        Event::Known(k) => Value::Object(k.body().clone()),
        Event::Opaque(_) => {
            return Err(Error::code(
                Code::E2103,
                format!("{} is opaque; it is not a file.version_added", row.event_id),
            ));
        }
    };
    let mut v = Version::from_added(&body)?;
    v.kept = row.kept();
    Ok(v)
}

/// Where a version's bytes are now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Bytes {
    /// On disk at the path, matching the pinned fingerprint.
    OnDisk(PathBuf),
    /// Kept in the record's object store.
    Kept(EventId),
}

/// Locate a version's bytes: the file on disk when its fingerprint matches
/// (hashed now), else the kept object, else `E3203`.
///
/// # Errors
/// `E3203` when neither is available; `Error::Io`.
pub fn locate_bytes(env: Env<'_>, identity: &Identity, row: &VersionRow) -> Result<Bytes> {
    let path = env.project.abs(&identity.path);
    if path.is_file() {
        let hashed = tm_fsfacts::hash::hash_file(&path, &env.settings.hash_settings())?;
        if hashed.fingerprint == row.fingerprint {
            return Ok(Bytes::OnDisk(path));
        }
    }
    if row.bytes_kept() && env.project.record.has_object(&row.fingerprint) {
        return Ok(Bytes::Kept(row.fingerprint.clone()));
    }
    Err(Error::code(
        Code::E3203,
        format!(
            "the bytes of {} are not available: {} does not match {} and they are not kept",
            row.handle, identity.path, row.fingerprint
        ),
    ))
}

/// Read a version's bytes (`files.bytes`).
///
/// # Errors
/// As [`locate_bytes`]; `Error::Record` for a kept object that does not
/// read.
pub fn version_bytes(env: Env<'_>, identity: &Identity, row: &VersionRow) -> Result<Vec<u8>> {
    match locate_bytes(env, identity, row)? {
        Bytes::OnDisk(p) => crate::read_file(&p),
        Bytes::Kept(fp) => {
            let mut r = env.project.record.open_object(&fp)?;
            let mut out = Vec::new();
            r.read_to_end(&mut out)
                .map_err(|e| Error::io(env.project.record.object_path(&fp), e))?;
            Ok(out)
        }
    }
}

/// A version's bytes as a file on disk: the path itself when it matches,
/// else the kept object inflated into `scratch` (for readers that take a
/// path: the table parser, the page counter).
///
/// # Errors
/// As [`version_bytes`].
pub fn version_file(
    env: Env<'_>,
    identity: &Identity,
    row: &VersionRow,
    scratch: &Path,
) -> Result<PathBuf> {
    match locate_bytes(env, identity, row)? {
        Bytes::OnDisk(p) => Ok(p),
        Bytes::Kept(fp) => {
            std::fs::create_dir_all(scratch).map_err(|e| Error::io(scratch, e))?;
            let name = Path::new(&identity.path)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "bytes".to_owned());
            let out = scratch.join(format!("{}-{name}", &fp.as_str()[7..23]));
            let bytes = version_bytes(env, identity, row)?;
            std::fs::write(&out, bytes).map_err(|e| Error::io(&out, e))?;
            Ok(out)
        }
    }
}

/// A cited table as Arrow with its spec: from the parse cache when present
/// (item 48), else parsed from the version's bytes and stored.
///
/// # Errors
/// `E6001` when the file has no parse spec; the parser's codes; as
/// [`version_bytes`].
pub fn table_of(
    env: Env<'_>,
    tx: &mut IndexTx<'_>,
    identity: &Identity,
    row: &VersionRow,
) -> Result<(RecordBatch, ParseSpec, EventId)> {
    let (spec, spec_fp) = spec_of(tx, identity.file)?.ok_or_else(|| {
        Error::code(
            Code::E6001,
            format!("{} has no parse spec (`tm table spec`)", identity.path),
        )
    })?;
    let cache = env.parse_cache();
    if let Some(batch) = cache.load(&row.fingerprint, &spec_fp)? {
        return Ok((batch, spec, spec_fp));
    }
    let scratch = env.project.parse_cache_dir().join("scratch");
    let path = version_file(env, identity, row, &scratch)?;
    let batch = tm_files::parse::parse(&path, &spec, &env.engine_settings())?;
    cache.store(&row.fingerprint, &spec_fp, &batch)?;
    Ok((batch, spec, spec_fp))
}

/// A registered version's citability (6.3): registered, with this version,
/// not retired.
///
/// # Errors
/// `E6001` otherwise; `Error::Index`.
pub fn citable(tx: &mut IndexTx<'_>, version: &VersionHandle) -> Result<(FileRow, VersionRow)> {
    let file = file_by_handle(tx, version.file)?
        .ok_or_else(|| Error::code(Code::E6001, format!("{version}: no file {}", version.file)))?;
    if file.identity.level != tm_files::Level::Registered {
        return Err(Error::code(
            Code::E6001,
            format!("{version} is tracked, not registered"),
        ));
    }
    if file.identity.retired {
        return Err(Error::code(Code::E6001, format!("{version} is retired")));
    }
    let row = version_by_handle(tx, version)?
        .ok_or_else(|| Error::code(Code::E6001, format!("{version} does not exist")))?;
    Ok((file, row))
}

/// The capability a Core function was given, as the actor.
#[must_use]
pub fn owner(cap: &OwnerCap) -> Actor<'_> {
    Actor::Owner(cap)
}
