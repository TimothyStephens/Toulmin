//! `file.register` (section 9; CUJ 1; item 74): identity, title,
//! description, source; the first version hashed in the foreground unless
//! `--defer-hash`; a provider checksum list compared once.

use std::path::Path;

use tm_files::checksum::{self, Compared, Entry};
use tm_files::{Candidate, Equivalence, Identity, Origin, Registration, Version};
use tm_formats::events::KnownEvent;
use tm_formats::id::{EventId, FileHandle, VersionHandle};
use tm_owner_cap::OwnerCap;

use crate::ops::{Core, Env, file_by_path, next_file_handle, owner};
use crate::{Code, Error, Result, stat_cache};

/// What `file.register` did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Registered {
    /// The identity.
    pub identity: Identity,
    /// The first version, unless deferred.
    pub version: Option<Version>,
    /// The checksum comparison, when a list was given.
    pub checksum: Option<Compared>,
    /// The event IDs written.
    pub events: Vec<EventId>,
}

/// The arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Args<'a> {
    /// The path, relative to the working directory or the repository root.
    pub path: &'a Path,
    /// Title, description, source.
    pub registration: Registration,
    /// A provider checksum list to compare once.
    pub checksums: Option<&'a Path>,
    /// Register the identity without a version (`--defer-hash`).
    pub defer_hash: bool,
}

/// Hash a path as a candidate version: a regular file, or a directory
/// walked with `manifest.ignore`.
///
/// # Errors
/// `E3001` when the path is neither; `Error::Io`.
pub fn candidate(env: Env<'_>, abs: &Path) -> Result<Candidate> {
    let hs = env.settings.hash_settings();
    if abs.is_dir() {
        Ok(Candidate::of_directory(
            abs,
            &env.settings.manifest_ignore(),
            &hs,
        )?)
    } else if abs.is_file() {
        Ok(Candidate::of_file(abs, &hs)?)
    } else {
        Err(Error::code(
            Code::E3001,
            format!(
                "{} is not a file or directory inside the project",
                abs.display()
            ),
        ))
    }
}

/// The entries a checksum list is compared with: the file itself under its
/// own name, or a directory manifest's entries.
fn entries(abs: &Path, rel: &str, cand: &Candidate) -> Vec<Entry> {
    match &cand.manifest {
        Some(m) => m
            .get("entries")
            .and_then(serde_json::Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|e| {
                        let path = e.get("path")?.as_str()?;
                        let fp = EventId::parse(e.get("fingerprint")?.as_str()?)?;
                        Some(Entry {
                            name: path.to_owned(),
                            path: abs.join(path),
                            fingerprint: fp,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default(),
        None => vec![Entry {
            name: Path::new(rel)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| rel.to_owned()),
            path: abs.to_path_buf(),
            fingerprint: cand.fingerprint.clone(),
        }],
    }
}

/// Compare a provider list with a version's entries (section 9's
/// `file.checksum_checked`; item 50).
///
/// # Errors
/// `E3006` list unreadable; `E3004` mismatch; `Error::Io`.
pub fn compare_checksums(
    list: &Path,
    abs: &Path,
    rel: &str,
    cand: &Candidate,
    file: FileHandle,
    version: &VersionHandle,
) -> Result<(Compared, KnownEvent)> {
    let text = crate::read_text(list)?;
    let lines = checksum::parse_checksums(&text)?;
    let compared = checksum::compare(&lines, &entries(abs, rel, cand))?;
    if let Some(e) = compared.refusal() {
        return Err(e.into());
    }
    let ev = compared.event(file, version)?;
    Ok((compared, ev))
}

/// `tm file register`.
///
/// # Errors
/// `E3001` outside project; `E3002` missing title; `E3003` non-UTF-8
/// path; `E3004` checksum mismatch; `E3006` checksum list unreadable;
/// `E3201` already registered; the lock's and the append's.
pub fn register(core: &mut Core, cap: &OwnerCap, args: &Args<'_>) -> Result<Registered> {
    let rel = core.relative(args.path)?;
    let abs = core.project.abs(&rel);
    if !abs.exists() {
        return Err(Error::code(
            Code::E3001,
            format!("{} does not exist", abs.display()),
        ));
    }
    {
        let mut tx = core.tx()?;
        if file_by_path(&mut tx, &rel)?.is_some() {
            return Err(Error::code(
                Code::E3201,
                format!("cannot register this file: {rel} is already registered"),
            ));
        }
        tx.rollback()?;
    }
    let mut locked = core.lock(|_, _| {})?;
    let handle = {
        let mut tx = locked.core.tx()?;
        let h = next_file_handle(&mut tx)?;
        tx.rollback()?;
        h
    };
    let (identity, ev_reg) = Identity::register(handle, &rel, &args.registration)?;
    let mut events = vec![ev_reg];
    let mut version = None;
    let mut checksum = None;
    let mut cached: Option<Candidate> = None;
    if !args.defer_hash {
        let cand = candidate(locked.core.env(), &abs)?;
        let (v, ev) = Version::next(
            handle,
            None,
            &cand,
            Origin::Registered,
            Equivalence::Different,
        )?
        .ok_or_else(|| Error::code(Code::E3203, "a first version always exists"))?;
        events.push(ev);
        if let Some(list) = args.checksums {
            let (c, ev) = compare_checksums(list, &abs, &rel, &cand, handle, &v.handle)?;
            checksum = Some(c);
            events.push(ev);
        }
        version = Some(v);
        cached = Some(cand);
    }
    let committed = locked.commit(&owner(cap), &events)?;
    if let (Some(cand), Some(v)) = (&cached, &version) {
        let at = locked.core.at.clone();
        let mut tx = locked.core.tx()?;
        stat_cache::put(
            &mut tx,
            &rel,
            Some(cand.snapshot),
            Some(&v.fingerprint),
            &at,
        )?;
        tx.commit()?;
    }
    locked.release()?;
    Ok(Registered {
        identity,
        version,
        checksum,
        events: committed.own().to_vec(),
    })
}
