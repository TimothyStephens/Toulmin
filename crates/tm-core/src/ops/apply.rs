//! `apply` (section 9; 11.15; 11.20; item 74): read an action file the
//! static view produced, refuse an item whose target changed since the
//! file's `chain_head` (`E8101`), list one line per item for the CLI's
//! confirmation, and commit the actions' events with `source:
//! static-view`. Gate A applies `claim.review` items only: a
//! `draft.promote` or `run.approve` item is `E8102`; a file that is I-JSON
//! but not of 11.15's shape, or an item that is not a well-formed instance
//! of its `op`, is `E8103`.

use std::path::Path;

use serde_json::Value;
use tm_claims::reviews::{Review, ReviewOutcome};
use tm_formats::canonical;
use tm_formats::envelope::SourceKind;
use tm_formats::events::KnownEvent;
use tm_formats::id::{ClaimRef, EventId};
use tm_index::Param;
use tm_owner_cap::OwnerCap;

use crate::ops::claim_check::report_fingerprint;
use crate::ops::{Core, owner, reports};
use crate::read::claims::claim_row;
use crate::{Code, Error, Result};

/// One item of the plan: what `tm apply` prints and asks about.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    /// `claim.review`.
    pub op: String,
    /// The target: the ref.
    pub target: String,
    /// The revision the action was made against.
    pub revision: String,
    /// The override the item sets, if any (none in gate A).
    pub override_: Option<String>,
    /// The event to write.
    pub event: KnownEvent,
}

/// The plan `tm apply` confirms.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    /// `exported_at`.
    pub exported_at: String,
    /// `chain_head`.
    pub chain_head: String,
    /// The items, in file order.
    pub items: Vec<Item>,
}

impl Plan {
    /// True when an item sets an override (`--yes-override` required).
    #[must_use]
    pub fn needs_override(&self) -> bool {
        self.items.iter().any(|i| i.override_.is_some())
    }

    /// One line per item: its target, revision, and any override.
    #[must_use]
    pub fn lines(&self) -> Vec<String> {
        self.items
            .iter()
            .map(|i| match &i.override_ {
                Some(o) => format!("{} {} @ {} (override: {o})", i.op, i.target, i.revision),
                None => format!("{} {} @ {}", i.op, i.target, i.revision),
            })
            .collect()
    }
}

/// The 11.15 actions gate A does not apply.
const NOT_IN_GATE_A: [&str; 2] = ["draft.promote", "run.approve"];

fn e8103(why: impl std::fmt::Display) -> Error {
    Error::code(Code::E8103, format!("malformed action file: {why}"))
}

/// Read and check an action file (11.15), without writing.
///
/// # Errors
/// `E1003` not I-JSON; `E1001` a float; `E8101` the target changed since
/// `chain_head`, or `chain_head` is not on the chain; `E8102` a
/// `draft.promote` or `run.approve` item; `E8103` a file not of 11.15's
/// shape or an item that is not a well-formed `claim.review`; `E6201`
/// unknown ref; `Error::Io`.
pub fn plan(core: &mut Core, file: &Path) -> Result<Plan> {
    let bytes = crate::read_file(file)?;
    let v = canonical::parse_bytes(&bytes)?;
    if v.get("actions").and_then(Value::as_u64) != Some(tm_formats::version::ACTIONS) {
        return Err(e8103("`actions` is not 1"));
    }
    let exported_at = v
        .get("exported_at")
        .and_then(Value::as_str)
        .ok_or_else(|| e8103("no `exported_at`"))?
        .to_owned();
    let chain_head = v
        .get("chain_head")
        .and_then(Value::as_str)
        .ok_or_else(|| e8103("no `chain_head`"))?
        .to_owned();
    let items_json = v
        .get("items")
        .and_then(Value::as_array)
        .ok_or_else(|| e8103("no `items` list"))?;
    let mut tx = core.index.begin()?;
    let on_chain = !tx
        .query_rows(
            "SELECT 1 FROM events WHERE id = ?",
            &[Param::from(chain_head.as_str())],
        )?
        .is_empty();
    if !on_chain {
        return Err(Error::code(
            Code::E8101,
            format!("chain_head {chain_head} is not on this chain; export the view again"),
        ));
    }
    let report_paths = reports(&mut tx)?;
    // What each item needs from the index, read first so the transaction
    // ends before the report files are hashed.
    struct Pending {
        op: String,
        claim_ref: ClaimRef,
        revision: EventId,
        outcome: ReviewOutcome,
        note: Option<String>,
        state: tm_claims::State,
        report_path: Option<String>,
    }
    let mut pending = Vec::new();
    for (i, item) in items_json.iter().enumerate() {
        let op = item.get("op").and_then(Value::as_str).unwrap_or_default();
        if NOT_IN_GATE_A.contains(&op) {
            return Err(Error::code(
                Code::E8102,
                format!("item {i} is {op}; gate A applies claim.review items only"),
            ));
        }
        if op != "claim.review" {
            return Err(e8103(format!(
                "item {i} has op {op:?}, not an 11.15 action"
            )));
        }
        let s = |k: &str| item.get(k).and_then(Value::as_str);
        let claim_ref = s("ref")
            .and_then(ClaimRef::parse)
            .ok_or_else(|| e8103(format!("item {i} has no valid `ref`")))?;
        let revision = s("revision")
            .and_then(EventId::parse)
            .ok_or_else(|| e8103(format!("item {i} has no valid `revision`")))?;
        let outcome = s("outcome")
            .and_then(ReviewOutcome::parse)
            .ok_or_else(|| e8103(format!("item {i} has no valid `outcome`")))?;
        let note = s("note").map(str::to_owned);
        let row = claim_row(&mut tx, claim_ref)?
            .ok_or_else(|| Error::code(Code::E6201, format!("item {i}: no claim {claim_ref}")))?;
        if row.current_revision != revision.as_str() {
            return Err(Error::code(
                Code::E8101,
                format!(
                    "item {i}: {claim_ref} was revised since the export ({} is current, the item names {revision})",
                    row.current_revision
                ),
            ));
        }
        let report_path = report_paths
            .iter()
            .find(|r| Some(r.report) == row.report)
            .map(|r| r.path.clone());
        pending.push(Pending {
            op: op.to_owned(),
            claim_ref,
            revision,
            outcome,
            note,
            state: row.state,
            report_path,
        });
    }
    tx.rollback()?;
    let mut items = Vec::new();
    for p in pending {
        let fp = match &p.report_path {
            Some(path) => report_fingerprint(core, path)?,
            None => tm_fsfacts::hash::fingerprint_of(b""),
        };
        let event = Review {
            claim_ref: p.claim_ref,
            revision: p.revision.clone(),
            outcome: p.outcome,
            note: p.note,
            report_fingerprint: fp,
        }
        .event(Some(p.state))?;
        items.push(Item {
            op: p.op,
            target: p.claim_ref.to_string(),
            revision: p.revision.as_str().to_owned(),
            override_: None,
            event,
        });
    }
    Ok(Plan {
        exported_at,
        chain_head,
        items,
    })
}

/// Commit a confirmed plan: one batch, `source: static-view`.
///
/// # Errors
/// The lock's and the append's.
pub fn commit(core: &mut Core, cap: &OwnerCap, plan: &Plan) -> Result<Vec<EventId>> {
    core.source.source = SourceKind::StaticView;
    let events: Vec<KnownEvent> = plan.items.iter().map(|i| i.event.clone()).collect();
    let mut locked = core.lock(|_, _| {})?;
    let committed = locked.commit(&owner(cap), &events)?;
    locked.release()?;
    core.source.source = SourceKind::Cli;
    Ok(committed.own().to_vec())
}
