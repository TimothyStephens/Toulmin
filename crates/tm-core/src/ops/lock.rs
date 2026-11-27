//! `lock.status` and `lock.break` (section 9; 3.1; item 74).

use tm_owner_cap::OwnerCap;
use tm_record::LockHolder;

use crate::ops::{Core, owner};
use crate::{Result, index};

/// `tm lock status`: the holder, if any.
///
/// # Errors
/// `Error::Io`.
pub fn status(core: &Core) -> Result<Option<LockHolder>> {
    Ok(core.record().lock_status()?)
}

/// `tm lock break`: wait one takeover period and take the lock if the
/// heartbeat did not change, recording `lock.taken_over`; the lock is
/// released at once.
///
/// # Errors
/// `E2201` when the heartbeat still changes; the Record's lock errors.
pub fn break_lock(
    core: &mut Core,
    cap: &OwnerCap,
    on_wait: impl FnMut(&LockHolder, std::time::Duration),
) -> Result<Option<LockHolder>> {
    let opts = core.settings.lock_options();
    let lock = core.project.record.break_lock(&opts, on_wait)?;
    index::sync(&mut core.index, &core.project.record)?;
    let displaced = lock.taken_over().cloned();
    let mut locked = super::Locked {
        core,
        read_in: super::read_in::ReadIn::default(),
        lock,
    };
    locked.commit(&owner(cap), &[])?;
    locked.release()?;
    Ok(displaced)
}
