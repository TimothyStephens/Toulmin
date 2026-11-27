//! `claim.supersede` (section 9; CUJ 5c; item 74): as `claim.add` for the
//! replacement, then `claim.superseded` linking the old ref to it, in one
//! batch (11.16).

use tm_claims::claim::superseded_event;
use tm_formats::id::ClaimRef;
use tm_owner_cap::OwnerCap;

use crate::ops::claim_add::{Added, Args, add_locked, build_content, validate};
use crate::ops::{Core, report_by_handle_or_path};
use crate::read::claims::claim_row;
use crate::{Code, Error, Result};

/// `tm claim supersede OLD` with `claim add`'s arguments for the
/// replacement.
///
/// # Errors
/// `E6201` unknown old ref; `E6202` an old ref already retracted or
/// superseded; `claim.add`'s codes; the
/// lock's and the append's.
pub fn supersede(core: &mut Core, cap: &OwnerCap, old: ClaimRef, args: &Args) -> Result<Added> {
    {
        let mut tx = core.tx()?;
        let row = claim_row(&mut tx, old)?
            .ok_or_else(|| Error::code(Code::E6201, format!("no claim {old}")))?;
        tx.rollback()?;
        if row.state.is_final() {
            return Err(Error::code(
                Code::E6202,
                format!(
                    "{old} is already {}; it cannot be superseded",
                    row.state.as_str()
                ),
            ));
        }
    }
    let report = report_by_handle_or_path(core, &args.report)?;
    let content = build_content(core, args)?;
    validate(core, &content)?;
    let mut locked = core.lock(|_, _| {})?;
    let added = add_locked(
        &mut locked,
        cap,
        args.expected_ref,
        args.kind,
        report.report,
        Some(old),
        content,
        |new| Ok(vec![superseded_event(old, new)?]),
    )?;
    locked.release()?;
    Ok(added)
}
