//! `file.keep`, `file.removable`, `file.restore`, `file.rename` (section 9;
//! section 6; item 74), each a 6.3 transition Files checks.

use tm_files::version::KeepReason;
use tm_files::{Identity, Version};
use tm_formats::id::{EventId, VersionHandle};
use tm_owner_cap::OwnerCap;

use crate::ops::{
    Bytes, Core, file_by_handle_or_path, latest_version, locate_bytes, owner, read_version,
    version_by_handle,
};
use crate::{Code, Error, Result, stat_cache};

/// What `file.keep` decided or did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Keep {
    /// The version is above `keep.warn_mib` and `confirm_large` was not
    /// given: nothing written; the CLI asks and calls again.
    NeedsConfirmation {
        /// The version's size in bytes.
        size: u64,
        /// The threshold in MiB.
        warn_mib: u64,
    },
    /// Kept.
    Kept {
        /// The version after the keep.
        version: Version,
        /// Compressed size in the object store (0 for fingerprint only).
        compressed_size: u64,
        /// The `file.kept` event.
        event: EventId,
    },
}

/// `tm file keep HANDLE [--fingerprint-only]` (D4; 5.5; 11.21 Objects).
/// `version` is `F<n>@v<m>` or `F<n>` (its latest version).
///
/// # Errors
/// `E3201` invalid transition (not registered, already kept, a directory
/// version); `E3202` above `keep.budget_mib`; `E3203` bytes missing; the
/// lock's and the append's.
pub fn keep(
    core: &mut Core,
    cap: &OwnerCap,
    version: &str,
    fingerprint_only: bool,
    confirm_large: bool,
) -> Result<Keep> {
    let (identity, row) = resolve_version(core, version)?;
    let v = read_version(core, &row)?;
    if v.manifest.is_some() {
        return Err(Error::code(
            Code::E3201,
            format!(
                "cannot keep this file: {} is a directory version; a manifest has no raw bytes to keep",
                row.handle
            ),
        ));
    }
    let mut state = identity.state();
    state.latest = Some(row.kept());
    // The transition is checked before any bytes move.
    state.apply(tm_files::Action::Keep { fingerprint_only })?;
    let warn_mib = core.settings.u64_or("keep.warn_mib", 1024);
    if !fingerprint_only && !confirm_large && v.size > warn_mib.saturating_mul(1_048_576) {
        return Ok(Keep::NeedsConfirmation {
            size: v.size,
            warn_mib,
        });
    }
    if let Some(budget) = core.settings.int("keep.budget_mib")
        && !fingerprint_only
    {
        let kept: u64 = {
            let mut tx = core.tx()?;
            let rows = tx.query_rows(
                "SELECT size FROM file_versions WHERE kept_object IS NOT NULL AND fingerprint_only = 0",
                &[],
            )?;
            tx.rollback()?;
            rows.iter().filter_map(|r| r.get(0).as_u64()).sum()
        };
        let budget_bytes = u64::try_from(budget).unwrap_or(0).saturating_mul(1_048_576);
        if kept.saturating_add(v.size) > budget_bytes {
            return Err(Error::code(
                Code::E3202,
                format!(
                    "keeping {} ({} bytes) would put the kept bytes at {} over the budget of {budget} MiB",
                    row.handle,
                    v.size,
                    kept.saturating_add(v.size)
                ),
            ));
        }
    }
    let mut locked = core.lock(|_, _| {})?;
    let compressed_size = if fingerprint_only {
        0
    } else {
        match locate_bytes(locked.core.env(), &identity, &row)? {
            Bytes::OnDisk(path) => {
                let mut f = std::fs::File::open(&path).map_err(|e| Error::io(&path, e))?;
                locked
                    .core
                    .project
                    .record
                    .keep(&mut f, &row.fingerprint)?
                    .compressed_size
            }
            Bytes::Kept(_) => {
                return Err(Error::code(
                    Code::E3201,
                    format!("cannot keep this file: {} is already kept", row.handle),
                ));
            }
        }
    };
    let (after, ev) = v.kept(
        state,
        &row.fingerprint,
        compressed_size,
        KeepReason::Owner,
        fingerprint_only,
    )?;
    let committed = locked.commit(&owner(cap), &[ev])?;
    locked.release()?;
    Ok(Keep::Kept {
        version: after,
        compressed_size,
        event: committed.own()[0].clone(),
    })
}

fn resolve_version(core: &mut Core, text: &str) -> Result<(Identity, super::VersionRow)> {
    if let Some(h) = VersionHandle::parse(text) {
        let row = file_by_handle_or_path(core, &h.file.to_string())?;
        let mut tx = core.tx()?;
        let v = version_by_handle(&mut tx, &h)?;
        tx.rollback()?;
        let v = v.ok_or_else(|| Error::code(Code::E6001, format!("{text} does not exist")))?;
        return Ok((row.identity, v));
    }
    let row = file_by_handle_or_path(core, text)?;
    let mut tx = core.tx()?;
    let v = latest_version(&mut tx, row.identity.file)?;
    tx.rollback()?;
    let v = v.ok_or_else(|| {
        Error::code(
            Code::E3201,
            format!("cannot keep this file: {text} has no version yet"),
        )
    })?;
    Ok((row.identity, v))
}

/// `tm file removable HANDLE`.
///
/// # Errors
/// `E3201` already removable; `E6001` unknown file; the lock's and the
/// append's.
pub fn removable(
    core: &mut Core,
    cap: &OwnerCap,
    handle_or_path: &str,
) -> Result<(Identity, EventId)> {
    let row = file_by_handle_or_path(core, handle_or_path)?;
    let (identity, ev) = row.identity.mark_removable()?;
    let mut locked = core.lock(|_, _| {})?;
    let committed = locked.commit(&owner(cap), &[ev])?;
    locked.release()?;
    Ok((identity, committed.own()[0].clone()))
}

/// `tm file restore HANDLE` (gate A reaches only its `E3201` refusal, since
/// nothing retires a file before gate C).
///
/// # Errors
/// `E3201` not retired; `E6001` unknown file; the lock's and the append's.
pub fn restore(
    core: &mut Core,
    cap: &OwnerCap,
    handle_or_path: &str,
) -> Result<(Identity, EventId)> {
    let row = file_by_handle_or_path(core, handle_or_path)?;
    let (identity, ev) = row.identity.restore()?;
    let mut locked = core.lock(|_, _| {})?;
    let committed = locked.commit(&owner(cap), &[ev])?;
    locked.release()?;
    Ok((identity, committed.own()[0].clone()))
}

/// `tm file rename FROM TO`: an explicit command that keeps versions
/// (6.3). The file is renamed on disk by the owner, before or after; the
/// stat cache row moves with it.
///
/// # Errors
/// `E3201` for an output or a retired identity; `E3001` for a path of the
/// wrong form or outside the project; `E6001` unknown file; the lock's and
/// the append's.
pub fn rename(
    core: &mut Core,
    cap: &OwnerCap,
    from: &str,
    to: &std::path::Path,
) -> Result<(Identity, EventId)> {
    let row = file_by_handle_or_path(core, from)?;
    let to_rel = core.relative(to)?;
    {
        let mut tx = core.tx()?;
        if super::file_by_path(&mut tx, &to_rel)?.is_some() {
            return Err(Error::code(
                Code::E3201,
                format!("cannot rename this file: {to_rel} is already registered"),
            ));
        }
        tx.rollback()?;
    }
    let (identity, ev) = row.identity.rename(&to_rel)?;
    let mut locked = core.lock(|_, _| {})?;
    let committed = locked.commit(&owner(cap), &[ev])?;
    let at = locked.core.at.clone();
    let mut tx = locked.core.tx()?;
    if let Some(old) = stat_cache::get(&mut tx, &row.identity.path)? {
        stat_cache::remove(&mut tx, &row.identity.path)?;
        stat_cache::put(
            &mut tx,
            &to_rel,
            old.snapshot,
            old.fingerprint.as_ref(),
            &at,
        )?;
    }
    tx.commit()?;
    locked.release()?;
    Ok((identity, committed.own()[0].clone()))
}
