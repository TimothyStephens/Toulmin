//! `status` (section 9; item 74; item 77): take the lock, read in, scan
//! the registered inputs against the node-local stat cache, version a hand
//! edit as `changed outside a run` (6.2, CUJ 7), rescan the spans, and
//! list the open flags with their next commands.

use serde_json::{Value, json};
use tm_files::{Origin, Version};
use tm_formats::id::{EventId, FileHandle, VersionHandle};
use tm_fsfacts::needs_rehash::{Context, Decision, decide};
use tm_fsfacts::racy::{Marker, is_racy};
use tm_fsfacts::snapshot::Snapshot;
use tm_owner_cap::OwnerCap;

use crate::ops::file_register::candidate;
use crate::ops::read_in::ReadIn;
use crate::ops::report_check::rescan;
use crate::ops::{Core, FileRow, Locked, files, latest_version, owner, read_version, spec_of};
use crate::read::Flag;
use crate::read::flags::compute;
use crate::{Result, stat_cache};

/// What the disk scan found.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Scan {
    /// Registered inputs looked at.
    pub scanned: u64,
    /// Files hashed.
    pub hashed: u64,
    /// New versions written, `changed outside a run`.
    pub new_versions: Vec<VersionHandle>,
    /// Paths whose stat changed above the inline threshold, left
    /// unconfirmed.
    pub unconfirmed: Vec<String>,
    /// Paths not on disk.
    pub missing: Vec<String>,
}

/// `tm status`'s result.
#[derive(Debug, Clone)]
pub struct Status {
    /// What read-in did.
    pub read_in: ReadIn,
    /// The scan.
    pub scan: Scan,
    /// Spans found across the registered reports.
    pub spans: usize,
    /// The open flags and labels.
    pub flags: Vec<Flag>,
    /// The events written (read-in's and the scan's).
    pub events: Vec<EventId>,
}

impl Status {
    /// The `--json` rendering.
    #[must_use]
    pub fn to_json(&self) -> Value {
        json!({
            "read_in": {
                "torn_tail": self.read_in.torn_tail.map(|t| t.body()),
                "taken_over": self.read_in.taken_over.as_ref().map(tm_record::LockHolder::to_json),
            },
            "scan": {
                "scanned": self.scan.scanned,
                "hashed": self.scan.hashed,
                "new_versions": self.scan.new_versions.iter().map(ToString::to_string).collect::<Vec<_>>(),
                "unconfirmed": self.scan.unconfirmed,
                "missing": self.scan.missing,
            },
            "spans": self.spans,
            "flags": self.flags.iter().map(Flag::to_json).collect::<Vec<_>>(),
            "events": self.events.iter().map(|e| e.as_str().to_owned()).collect::<Vec<_>>(),
        })
    }
}

/// One scanned path's outcome.
enum Found {
    Unchanged,
    Hashed(tm_files::Candidate),
    Unconfirmed(Snapshot),
    Missing,
}

fn look(
    locked: &mut Locked<'_>,
    f: &FileRow,
    marker: &Marker,
    context: Context,
    refresh: bool,
) -> Result<Found> {
    let abs = locked.core.project.abs(&f.identity.path);
    let current = match Snapshot::take(&abs) {
        Ok(s) => s,
        Err(tm_fsfacts::Error::Io { source, .. })
            if source.kind() == std::io::ErrorKind::NotFound =>
        {
            return Ok(Found::Missing);
        }
        Err(e) => return Err(e.into()),
    };
    let previous = if refresh {
        None
    } else {
        let mut tx = locked.core.tx()?;
        let e = stat_cache::get(&mut tx, &f.identity.path)?;
        tx.rollback()?;
        e.and_then(|e| e.fingerprint.and_then(|_| e.snapshot))
    };
    let settings = locked.core.settings.rehash_settings();
    let racy = is_racy(&current, marker);
    Ok(
        match decide(previous.as_ref(), &current, racy, &settings, context) {
            Decision::Unchanged => Found::Unchanged,
            Decision::HashNow => Found::Hashed(candidate(locked.core.env(), &abs)?),
            Decision::Unconfirmed => Found::Unconfirmed(current),
        },
    )
}

/// The `changed outside a run` version for a candidate that differs from
/// the latest version, with the three equivalences (item 45).
fn hand_edit(
    locked: &mut Locked<'_>,
    file: FileHandle,
    previous: &super::VersionRow,
    cand: &tm_files::Candidate,
    abs: &std::path::Path,
) -> Result<Option<(Version, tm_formats::events::KnownEvent)>> {
    let prev = read_version(locked.core, previous)?;
    if prev.fingerprint == cand.fingerprint {
        return Ok(None);
    }
    // The second equivalence needs both tables parsed under the spec.
    let mut tables = None;
    if cand.manifest.is_none() {
        let mut tx = locked.core.tx()?;
        let spec = spec_of(&mut tx, file)?;
        tx.rollback()?;
        if let Some((spec, spec_fp)) = spec {
            let cache = locked.core.parse_cache();
            if let Some(prev_batch) = cache.load(&previous.fingerprint, &spec_fp)?
                && let Ok(new_batch) =
                    tm_files::parse::parse(abs, &spec, &locked.core.engine_settings())
            {
                cache.store(&cand.fingerprint, &spec_fp, &new_batch)?;
                tables = Some((prev_batch, new_batch, spec));
            }
        }
    }
    let eq =
        tm_files::version::equivalence(&prev, cand, tables.as_ref().map(|(p, c, s)| (p, c, s)))?;
    Ok(Version::next(
        file,
        Some(&prev),
        cand,
        Origin::ChangedOutsideRun,
        eq,
    )?)
}

/// `tm status [--refresh] [--hash]`: `--refresh` ignores the stat cache's
/// snapshots; `--hash` hashes stat-changed files above the inline
/// threshold too (the `Run` context of item 30).
///
/// # Errors
/// The lock's and the append's; `Error::Io` for a path that stats but does
/// not read; `Error::Index`.
pub fn status(core: &mut Core, cap: &OwnerCap, refresh: bool, hash: bool) -> Result<Status> {
    let mut locked = core.lock(|_, _| {})?;
    let read_in = locked.read_in.clone();
    let context = if hash { Context::Run } else { Context::Scan };
    let rows = {
        let mut tx = locked.core.tx()?;
        let r = files(&mut tx)?;
        tx.rollback()?;
        r
    };
    let marker = Marker::touch(&locked.core.project.repo_root)?;
    let mut scan = Scan::default();
    let mut events = Vec::new();
    let mut cache_writes: Vec<(String, Option<Snapshot>, Option<EventId>)> = Vec::new();
    for f in &rows {
        if f.identity.level != tm_files::Level::Registered {
            continue;
        }
        let latest = {
            let mut tx = locked.core.tx()?;
            let l = latest_version(&mut tx, f.identity.file)?;
            tx.rollback()?;
            l
        };
        let Some(latest) = latest else {
            continue;
        };
        scan.scanned += 1;
        match look(&mut locked, f, &marker, context, refresh)? {
            Found::Unchanged => {}
            Found::Missing => {
                scan.missing.push(f.identity.path.clone());
                cache_writes.push((
                    f.identity.path.clone(),
                    None,
                    Some(latest.fingerprint.clone()),
                ));
            }
            Found::Unconfirmed(snapshot) => {
                scan.unconfirmed.push(f.identity.path.clone());
                cache_writes.push((f.identity.path.clone(), Some(snapshot), None));
            }
            Found::Hashed(cand) => {
                scan.hashed += 1;
                let abs = locked.core.project.abs(&f.identity.path);
                match hand_edit(&mut locked, f.identity.file, &latest, &cand, &abs)? {
                    Some((v, ev)) => {
                        scan.new_versions.push(v.handle);
                        cache_writes.push((
                            f.identity.path.clone(),
                            Some(cand.snapshot),
                            Some(v.fingerprint),
                        ));
                        events.push(ev);
                    }
                    None => cache_writes.push((
                        f.identity.path.clone(),
                        Some(cand.snapshot),
                        Some(latest.fingerprint.clone()),
                    )),
                }
            }
        }
    }
    let committed = locked.commit(&owner(cap), &events)?;
    let at = locked.core.at.clone();
    {
        let mut tx = locked.core.tx()?;
        for (path, snapshot, fp) in &cache_writes {
            stat_cache::put(&mut tx, path, *snapshot, fp.as_ref(), &at)?;
        }
        tx.commit()?;
    }
    let spans = rescan(locked.core)?.len();
    let flags = compute(locked.core)?;
    locked.release()?;
    Ok(Status {
        read_in,
        scan,
        spans,
        flags,
        events: committed.ids,
    })
}
