//! `claim.review` (section 9; 5.4; item 74): a human line of evidence on a
//! claim's current revision, with the registered report's fingerprint at
//! review time; it never changes a verdict.

use tm_claims::reviews::{Review, ReviewOutcome};
use tm_formats::id::{ClaimRef, EventId};
use tm_owner_cap::OwnerCap;

use crate::ops::claim_check::report_fingerprint;
use crate::ops::{Core, owner, reports};
use crate::read::claims::claim_row;
use crate::{Code, Error, Result};

/// `tm claim review REF --outcome confirmed|disputed|question [--note N]`.
///
/// # Errors
/// `E6201` unknown ref; `Error::Io` when the report file does not read;
/// the lock's and the append's.
pub fn review(
    core: &mut Core,
    cap: &OwnerCap,
    claim_ref: ClaimRef,
    outcome: ReviewOutcome,
    note: Option<String>,
) -> Result<EventId> {
    let (row, report_path) = {
        let mut tx = core.tx()?;
        let row = claim_row(&mut tx, claim_ref)?
            .ok_or_else(|| Error::code(Code::E6201, format!("no claim {claim_ref}")))?;
        let path = reports(&mut tx)?
            .into_iter()
            .find(|r| Some(r.report) == row.report)
            .map(|r| r.path);
        tx.rollback()?;
        (row, path)
    };
    let fp = match report_path {
        Some(p) => report_fingerprint(core, &p)?,
        None => tm_fsfacts::hash::fingerprint_of(b""),
    };
    let revision = EventId::parse(&row.current_revision)
        .ok_or_else(|| Error::code(Code::E2103, "claims.current_revision is not an ID"))?;
    let ev = Review {
        claim_ref,
        revision,
        outcome,
        note,
        report_fingerprint: fp,
    }
    .event(Some(row.state))?;
    let mut locked = core.lock(|_, _| {})?;
    let committed = locked.commit(&owner(cap), &[ev])?;
    locked.release()?;
    Ok(committed.own()[0].clone())
}
