//! `claim.update_text` (section 9; CUJ 5a; item 74): record the sentence
//! the report now holds for the ref (its first span in file order), re-run
//! both checks, and so clear `text drifted`.

use tm_claims::claim::revised_event;
use tm_formats::id::{ClaimRef, EventId};
use tm_owner_cap::OwnerCap;

use crate::ops::claim_check::{Checked, check};
use crate::ops::report_check::{rescan, spans_of_report};
use crate::ops::{Core, owner, reports};
use crate::read::claims::{claim_row, revision_content};
use crate::{Code, Error, Result};

/// What `claim.update_text` did.
#[derive(Debug, Clone)]
pub struct Updated {
    /// The new sentence.
    pub sentence: String,
    /// The `claim.revised` event.
    pub revised: EventId,
    /// The `claim.checked` event.
    pub checked_event: EventId,
    /// The check.
    pub check: Checked,
}

/// `tm claim update-text REF`.
///
/// # Errors
/// `E6201` unknown ref; `E6202` a retracted or superseded claim; `E6301`
/// no span for the ref in
/// the registered report; the check's codes; the lock's and the append's.
pub fn update_text(core: &mut Core, cap: &OwnerCap, claim_ref: ClaimRef) -> Result<Updated> {
    let (row, content, report_path) = {
        let mut tx = core.tx()?;
        let row = claim_row(&mut tx, claim_ref)?
            .ok_or_else(|| Error::code(Code::E6201, format!("no claim {claim_ref}")))?;
        let content = revision_content(&mut tx, &row.current_revision)?;
        let path = reports(&mut tx)?
            .into_iter()
            .find(|r| Some(r.report) == row.report)
            .map(|r| r.path);
        tx.rollback()?;
        (row, content, path)
    };
    if row.state.is_final() {
        return Err(Error::code(
            Code::E6202,
            format!(
                "{claim_ref} is {}; its text is not updated",
                row.state.as_str()
            ),
        ));
    }
    let report_path = report_path.ok_or_else(|| {
        Error::code(
            Code::E6301,
            format!("{claim_ref}'s report is not registered"),
        )
    })?;
    let spans = spans_of_report(core, &report_path)?;
    let span = spans
        .into_iter()
        .find(|s| s.claim_ref == claim_ref)
        .ok_or_else(|| {
            Error::code(
                Code::E6301,
                format!("{report_path} has no span with ref={claim_ref}"),
            )
        })?;
    let mut next = content;
    next.sentence = span.text;
    let revised = revised_event(claim_ref, &next)?;
    let mut locked = core.lock(|_, _| {})?;
    let actor = owner(cap);
    let ids = locked.preview_ids(&actor, std::slice::from_ref(&revised))?;
    let limit = locked.core.settings.rows_per_page();
    let checked = check(locked.core, claim_ref, &ids[0], row.kind, &next, limit)?;
    let committed = locked.commit(&actor, &[revised, checked.event.clone()])?;
    rescan(locked.core)?;
    locked.release()?;
    Ok(Updated {
        sentence: next.sentence,
        revised: committed.own()[0].clone(),
        checked_event: committed.own()[1].clone(),
        check: checked,
    })
}
