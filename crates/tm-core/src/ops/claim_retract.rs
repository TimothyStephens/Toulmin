//! `claim.retract` (section 9; CUJ 5b; item 74): withdraw a claim with a
//! reason; it stays visible with the `retracted` label.

use tm_claims::claim::retracted_event;
use tm_formats::id::{ClaimRef, EventId};
use tm_owner_cap::OwnerCap;

use crate::ops::report_check::rescan;
use crate::ops::{Core, owner};
use crate::read::claims::claim_row;
use crate::{Code, Error, Result};

/// `tm claim retract REF --reason R`.
///
/// # Errors
/// `E6201` unknown ref or a claim already retracted or superseded; `E1002`
/// an empty reason; the lock's and the append's.
pub fn retract(
    core: &mut Core,
    cap: &OwnerCap,
    claim_ref: ClaimRef,
    reason: &str,
) -> Result<EventId> {
    let mut tx = core.tx()?;
    let row = claim_row(&mut tx, claim_ref)?
        .ok_or_else(|| Error::code(Code::E6201, format!("no claim {claim_ref}")))?;
    tx.rollback()?;
    if row.state.is_final() {
        return Err(Error::code(
            Code::E6201,
            format!("{claim_ref} is already {}", row.state.as_str()),
        ));
    }
    let ev = retracted_event(claim_ref, reason)?;
    let mut locked = core.lock(|_, _| {})?;
    let committed = locked.commit(&owner(cap), &[ev])?;
    rescan(locked.core)?;
    locked.release()?;
    Ok(committed.own()[0].clone())
}
