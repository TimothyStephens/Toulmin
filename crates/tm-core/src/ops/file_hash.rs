//! `file.hash` (section 9; item 74): the explicit hash step behind
//! `--defer-hash`, or `--again` on a file that has a version.

use std::path::Path;

use tm_files::checksum::Compared;
use tm_files::{Action, Equivalence, Origin, Version};
use tm_formats::id::EventId;
use tm_owner_cap::OwnerCap;

use crate::ops::file_register::{candidate, compare_checksums};
use crate::ops::{Core, file_by_handle_or_path, latest_version, owner, read_version};
use crate::{Code, Error, Result, stat_cache};

/// What `file.hash` did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hashed {
    /// The version added, `None` when `--again` found the same bytes.
    pub version: Option<Version>,
    /// The checksum comparison, when a list was given.
    pub checksum: Option<Compared>,
    /// The event IDs written.
    pub events: Vec<EventId>,
}

/// `tm file hash HANDLE [--again] [--checksums FILE]`.
///
/// # Errors
/// `E3005` already hashed (without `--again`); `E3201` not registered;
/// `E6001` unknown file; `E3004`, `E3006` for the list; the lock's and the
/// append's.
pub fn hash(
    core: &mut Core,
    cap: &OwnerCap,
    handle_or_path: &str,
    again: bool,
    checksums: Option<&Path>,
) -> Result<Hashed> {
    let row = file_by_handle_or_path(core, handle_or_path)?;
    let identity = row.identity.clone();
    let mut locked = core.lock(|_, _| {})?;
    let latest = {
        let mut tx = locked.core.tx()?;
        let l = latest_version(&mut tx, identity.file)?;
        tx.rollback()?;
        l
    };
    let mut state = identity.state();
    state.latest = latest.as_ref().map(super::VersionRow::kept);
    state.apply(Action::Hash { again })?;
    let previous = match &latest {
        Some(l) => Some(read_version(locked.core, l)?),
        None => None,
    };
    let abs = locked.core.project.abs(&identity.path);
    if !abs.exists() {
        return Err(Error::code(
            Code::E3203,
            format!("{} is not on disk", abs.display()),
        ));
    }
    let cand = candidate(locked.core.env(), &abs)?;
    let equivalence = match &previous {
        Some(p) => tm_files::version::equivalence(p, &cand, None)?,
        None => Equivalence::Different,
    };
    let next = Version::next(
        identity.file,
        previous.as_ref(),
        &cand,
        Origin::Registered,
        equivalence,
    )?;
    let mut events = Vec::new();
    let mut version = None;
    let mut checksum = None;
    let confirmed = match next {
        Some((v, ev)) => {
            events.push(ev);
            let handle = v.handle;
            if let Some(list) = checksums {
                let (c, ev) =
                    compare_checksums(list, &abs, &identity.path, &cand, identity.file, &handle)?;
                checksum = Some(c);
                events.push(ev);
            }
            let fp = v.fingerprint.clone();
            version = Some(v);
            fp
        }
        None => {
            // Same bytes as the latest version: a touch is not a version.
            let p = previous
                .as_ref()
                .ok_or_else(|| Error::code(Code::E3203, "no version and no change"))?;
            if let Some(list) = checksums {
                let (c, ev) =
                    compare_checksums(list, &abs, &identity.path, &cand, identity.file, &p.handle)?;
                checksum = Some(c);
                events.push(ev);
            }
            p.fingerprint.clone()
        }
    };
    let committed = locked.commit(&owner(cap), &events)?;
    let at = locked.core.at.clone();
    let mut tx = locked.core.tx()?;
    stat_cache::put(
        &mut tx,
        &identity.path,
        Some(cand.snapshot),
        Some(&confirmed),
        &at,
    )?;
    tx.commit()?;
    locked.release()?;
    Ok(Hashed {
        version,
        checksum,
        events: committed.own().to_vec(),
    })
}
