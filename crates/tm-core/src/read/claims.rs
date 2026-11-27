//! `claims.list`, `claims.show`, `claims.evidence`, `claims.citing_rows`,
//! `claims.history` (section 10; item 78), and the index readers the
//! claim operations share.

use serde_json::{Value, json};
use tm_claims::claim::{Content, Kind, State};
use tm_formats::citation::Citation;
use tm_formats::events::{EventType, KnownEvent};
use tm_formats::id::{ClaimRef, ReportHandle, VersionHandle};
use tm_formats::typed_value::TypedValue;
use tm_index::{IndexTx, Param, Row};

use crate::ops::Core;
use crate::read::{Flag, flags, list};
use crate::{Code, Error, Result};

/// A `claims` row (11.17).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimRow {
    /// The ref.
    pub claim_ref: ClaimRef,
    /// The claim's ID (its `claim.added`'s).
    pub id: String,
    /// The kind.
    pub kind: Kind,
    /// The state.
    pub state: State,
    /// `created_event`.
    pub created_event: String,
    /// `current_revision`.
    pub current_revision: String,
    /// The report.
    pub report: Option<ReportHandle>,
    /// `supersedes`.
    pub supersedes: Option<ClaimRef>,
    /// `superseded_by`.
    pub superseded_by: Option<ClaimRef>,
}

const CLAIM_COLUMNS: &str =
    "ref, id, kind, state, created_event, current_revision, report, supersedes, superseded_by";

fn claim_of(r: &Row) -> Option<ClaimRow> {
    Some(ClaimRow {
        claim_ref: ClaimRef::parse(r.text(0)?)?,
        id: r.text(1)?.to_owned(),
        kind: Kind::parse(r.text(2)?)?,
        state: State::parse(r.text(3)?)?,
        created_event: r.text(4)?.to_owned(),
        current_revision: r.text(5)?.to_owned(),
        report: r.text(6).and_then(ReportHandle::parse),
        supersedes: r.text(7).and_then(ClaimRef::parse),
        superseded_by: r.text(8).and_then(ClaimRef::parse),
    })
}

/// The `claims` row for a ref.
///
/// # Errors
/// `Error::Index`.
pub fn claim_row(tx: &mut IndexTx<'_>, claim_ref: ClaimRef) -> Result<Option<ClaimRow>> {
    let rows = tx.query_rows(
        &format!("SELECT {CLAIM_COLUMNS} FROM claims WHERE ref = ?"),
        &[Param::from(claim_ref.to_string())],
    )?;
    Ok(rows.first().and_then(claim_of))
}

/// Every `claims` row, by ref number.
///
/// # Errors
/// `Error::Index`.
pub fn claim_rows(tx: &mut IndexTx<'_>) -> Result<Vec<ClaimRow>> {
    let rows = tx.query_rows(
        &format!("SELECT {CLAIM_COLUMNS} FROM claims ORDER BY CAST(substr(ref, 2) AS INTEGER)"),
        &[],
    )?;
    Ok(rows.iter().filter_map(claim_of).collect())
}

/// A `claim_revisions` row as its content.
///
/// # Errors
/// `E2103` when the revision is not in the index; `Error::Index`.
pub fn revision_content(tx: &mut IndexTx<'_>, event_id: &str) -> Result<Content> {
    let rows = tx.query_rows(
        "SELECT ref, sentence, cited, citations, steps, assertion, qualifiers FROM claim_revisions WHERE event_id = ?",
        &[Param::from(event_id)],
    )?;
    let r = rows.first().ok_or_else(|| {
        Error::code(
            Code::E2103,
            format!("revision {event_id} is not in the index"),
        )
    })?;
    content_of(r, 1)
}

/// A content from a `claim_revisions` row whose `sentence` is column `i`
/// (then `cited`, `citations`, `steps`, `assertion`, `qualifiers`).
fn content_of(r: &Row, i: usize) -> Result<Content> {
    let json = |j: usize| -> Value {
        r.text(j)
            .and_then(|t| serde_json::from_str(t).ok())
            .unwrap_or(Value::Null)
    };
    let cited = json(i + 1)
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().and_then(VersionHandle::parse))
                .collect()
        })
        .unwrap_or_default();
    let citations = json(i + 2)
        .as_array()
        .map(|a| {
            a.iter()
                .enumerate()
                .filter_map(|(k, v)| Citation::from_json(v, &format!("/citations/{k}")).ok())
                .collect()
        })
        .unwrap_or_default();
    let opt = |j: usize| -> Option<Value> {
        let v = json(j);
        (!v.is_null()).then_some(v)
    };
    Ok(Content {
        sentence: r.text(i).unwrap_or_default().to_owned(),
        cited,
        citations,
        steps: opt(i + 3),
        assertion: opt(i + 4),
        qualifiers: r.text(i + 5).map(str::to_owned),
    })
}

/// A stored verdict (`verdicts` row).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerdictRow {
    /// The `claim.checked` event.
    pub event_id: String,
    /// The revision checked.
    pub revision: String,
    /// `against`, as handles.
    pub against: Vec<String>,
    /// `evidence_outcome`.
    pub evidence_outcome: String,
    /// `assertion_outcome`.
    pub assertion_outcome: Option<String>,
    /// The whole body.
    pub body: Value,
    /// `at`.
    pub at: String,
}

/// A ref's verdicts, oldest first.
///
/// # Errors
/// `Error::Index`.
pub fn verdicts_of(tx: &mut IndexTx<'_>, claim_ref: ClaimRef) -> Result<Vec<VerdictRow>> {
    let rows = tx.query_rows(
        "SELECT event_id, revision, against, evidence_outcome, assertion_outcome, body, at FROM verdicts WHERE ref = ? ORDER BY at, rowid",
        &[Param::from(claim_ref.to_string())],
    )?;
    Ok(rows
        .iter()
        .map(|r| VerdictRow {
            event_id: r.text(0).unwrap_or_default().to_owned(),
            revision: r.text(1).unwrap_or_default().to_owned(),
            against: serde_json::from_str::<Value>(r.text(2).unwrap_or("[]"))
                .ok()
                .and_then(|v| {
                    v.as_array().map(|a| {
                        a.iter()
                            .filter_map(|x| x.as_str().map(str::to_owned))
                            .collect()
                    })
                })
                .unwrap_or_default(),
            evidence_outcome: r.text(3).unwrap_or_default().to_owned(),
            assertion_outcome: r.text(4).map(str::to_owned),
            body: serde_json::from_str(r.text(5).unwrap_or("null")).unwrap_or(Value::Null),
            at: r.text(6).unwrap_or_default().to_owned(),
        })
        .collect())
}

/// The latest verdict of a ref's current revision, if any.
///
/// # Errors
/// `Error::Index`.
pub fn current_verdict(tx: &mut IndexTx<'_>, row: &ClaimRow) -> Result<Option<VerdictRow>> {
    Ok(verdicts_of(tx, row.claim_ref)?
        .into_iter()
        .filter(|v| v.revision == row.current_revision)
        .last())
}

/// A review (`reviews` row).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewRow {
    /// The `claim.reviewed` event.
    pub event_id: String,
    /// The revision reviewed.
    pub revision: String,
    /// The outcome.
    pub outcome: String,
    /// The note.
    pub note: Option<String>,
    /// `at`.
    pub at: String,
}

/// A ref's reviews, oldest first.
///
/// # Errors
/// `Error::Index`.
pub fn reviews_of(tx: &mut IndexTx<'_>, claim_ref: ClaimRef) -> Result<Vec<ReviewRow>> {
    let rows = tx.query_rows(
        "SELECT event_id, revision, outcome, note, at FROM reviews WHERE ref = ? ORDER BY at, rowid",
        &[Param::from(claim_ref.to_string())],
    )?;
    Ok(rows
        .iter()
        .map(|r| ReviewRow {
            event_id: r.text(0).unwrap_or_default().to_owned(),
            revision: r.text(1).unwrap_or_default().to_owned(),
            outcome: r.text(2).unwrap_or_default().to_owned(),
            note: r.text(3).map(str::to_owned),
            at: r.text(4).unwrap_or_default().to_owned(),
        })
        .collect())
}

/// A ref's revisions, oldest first: `(event_id, at, content)`.
///
/// # Errors
/// `Error::Index`.
pub fn revisions_of(
    tx: &mut IndexTx<'_>,
    claim_ref: ClaimRef,
) -> Result<Vec<(String, String, Content)>> {
    let rows = tx.query_rows(
        "SELECT event_id, at, sentence, cited, citations, steps, assertion, qualifiers FROM claim_revisions WHERE ref = ? ORDER BY at, rowid",
        &[Param::from(claim_ref.to_string())],
    )?;
    rows.iter()
        .map(|r| {
            Ok((
                r.text(0).unwrap_or_default().to_owned(),
                r.text(1).unwrap_or_default().to_owned(),
                content_of(r, 2)?,
            ))
        })
        .collect()
}

/// `claims.list`'s filters (section 10). Every field is optional and
/// narrows the list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListFilter {
    /// A ref prefix (`C1` matches `C1`, `C10`, ...).
    pub ref_prefix: Option<String>,
    /// A state.
    pub state: Option<State>,
    /// A kind.
    pub kind: Option<Kind>,
    /// A cited version.
    pub cited: Option<VersionHandle>,
    /// A flag name the claim carries.
    pub flag: Option<String>,
    /// A report.
    pub report: Option<ReportHandle>,
}

/// One claim as `claims.list` returns it.
#[derive(Debug, Clone)]
pub struct Listed {
    /// The row.
    pub row: ClaimRow,
    /// The current revision's content.
    pub content: Content,
    /// The latest verdict of the current revision.
    pub verdict: Option<VerdictRow>,
    /// Reviews.
    pub reviews: Vec<ReviewRow>,
    /// Open flags and labels on the claim.
    pub flags: Vec<Flag>,
}

fn content_json(c: &Content) -> Value {
    json!({
        "sentence": c.sentence,
        "cited": c.cited.iter().map(ToString::to_string).collect::<Vec<_>>(),
        "citations": c.citations.iter().map(Citation::to_json).collect::<Vec<_>>(),
        "steps": c.steps,
        "assertion": c.assertion,
        "qualifiers": c.qualifiers,
    })
}

fn verdict_summary(v: &VerdictRow) -> Value {
    json!({
        "event": v.event_id,
        "revision": v.revision,
        "against": v.against,
        "evidence": v.evidence_outcome,
        "assertion": v.assertion_outcome,
        "at": v.at,
    })
}

fn review_json(r: &ReviewRow) -> Value {
    json!({
        "event": r.event_id,
        "revision": r.revision,
        "outcome": r.outcome,
        "note": r.note,
        "at": r.at,
    })
}

impl Listed {
    /// The JSON shape of one list entry.
    #[must_use]
    pub fn to_json(&self) -> Value {
        json!({
            "ref": self.row.claim_ref.to_string(),
            "id": self.row.id,
            "kind": self.row.kind.as_str(),
            "state": self.row.state.as_str(),
            "report": self.row.report.map(|r| r.to_string()),
            "supersedes": self.row.supersedes.map(|r| r.to_string()),
            "superseded_by": self.row.superseded_by.map(|r| r.to_string()),
            "current_revision": self.row.current_revision,
            "sentence": self.content.sentence,
            "cited": self.content.cited.iter().map(ToString::to_string).collect::<Vec<_>>(),
            "verdict": self.verdict.as_ref().map(verdict_summary),
            "reviews": self.reviews.iter().map(review_json).collect::<Vec<_>>(),
            "flags": self.flags.iter().map(Flag::to_json).collect::<Vec<_>>(),
        })
    }
}

/// `claims.list`.
///
/// # Errors
/// `Error::Index`.
pub fn claims_list(core: &mut Core, filter: &ListFilter) -> Result<Vec<Listed>> {
    let all_flags = flags::compute(core)?;
    let mut tx = core.index.begin()?;
    let mut out = Vec::new();
    for row in claim_rows(&mut tx)? {
        if let Some(p) = &filter.ref_prefix
            && !row.claim_ref.to_string().starts_with(p.as_str())
        {
            continue;
        }
        if filter.state.is_some_and(|s| s != row.state)
            || filter.kind.is_some_and(|k| k != row.kind)
            || filter.report.is_some_and(|r| Some(r) != row.report)
        {
            continue;
        }
        let content = revision_content(&mut tx, &row.current_revision)?;
        if let Some(v) = &filter.cited
            && !content.cited.contains(v)
        {
            continue;
        }
        let subject = row.claim_ref.to_string();
        let claim_flags: Vec<Flag> = all_flags
            .iter()
            .filter(|f| f.subject_kind == "claim" && f.subject == subject)
            .cloned()
            .collect();
        if let Some(name) = &filter.flag
            && !claim_flags.iter().any(|f| &f.name == name)
        {
            continue;
        }
        let verdict = current_verdict(&mut tx, &row)?;
        let reviews = reviews_of(&mut tx, row.claim_ref)?;
        out.push(Listed {
            row,
            content,
            verdict,
            reviews,
            flags: claim_flags,
        });
    }
    tx.rollback()?;
    Ok(out)
}

/// `claims.list` as JSON.
///
/// # Errors
/// `Error::Index`.
pub fn claims_list_json(core: &mut Core, filter: &ListFilter) -> Result<Value> {
    Ok(list(claims_list(core, filter)?.iter().map(Listed::to_json)))
}

/// `claims.show`: the full claim, every revision, every verdict with its
/// SQL and versions, reviews, flags, and spans.
///
/// # Errors
/// `E6201` unknown ref; `Error::Index`.
pub fn claims_show(core: &mut Core, claim_ref: ClaimRef) -> Result<Value> {
    let all_flags = flags::compute(core)?;
    let mut tx = core.index.begin()?;
    let row = claim_row(&mut tx, claim_ref)?
        .ok_or_else(|| Error::code(Code::E6201, format!("no claim {claim_ref}")))?;
    let revisions = revisions_of(&mut tx, claim_ref)?;
    let verdicts = verdicts_of(&mut tx, claim_ref)?;
    let reviews = reviews_of(&mut tx, claim_ref)?;
    let spans = tx.query_rows(
        "SELECT report, line, pos, sentence, inline_code, section_kind, attributes FROM spans WHERE ref = ? ORDER BY report, line, pos",
        &[Param::from(claim_ref.to_string())],
    )?;
    tx.rollback()?;
    let subject = claim_ref.to_string();
    let current = revisions
        .last()
        .map(|(_, _, c)| c.clone())
        .unwrap_or_else(|| Content {
            sentence: String::new(),
            cited: Vec::new(),
            citations: Vec::new(),
            steps: None,
            assertion: None,
            qualifiers: None,
        });
    Ok(json!({
        "ref": subject,
        "id": row.id,
        "kind": row.kind.as_str(),
        "state": row.state.as_str(),
        "report": row.report.map(|r| r.to_string()),
        "supersedes": row.supersedes.map(|r| r.to_string()),
        "superseded_by": row.superseded_by.map(|r| r.to_string()),
        "current_revision": row.current_revision,
        "current": content_json(&current),
        "revisions": revisions.iter().map(|(id, at, c)| json!({"event": id, "at": at, "content": content_json(c)})).collect::<Vec<_>>(),
        "verdicts": verdicts.iter().map(|v| json!({"event": v.event_id, "at": v.at, "body": v.body})).collect::<Vec<_>>(),
        "reviews": reviews.iter().map(review_json).collect::<Vec<_>>(),
        "flags": all_flags.iter().filter(|f| (f.subject_kind == "claim" && f.subject == subject) || (f.subject_kind == "span" && f.reason.contains(&format!(" {subject} ")))).map(Flag::to_json).collect::<Vec<_>>(),
        "spans": spans.iter().map(|s| json!({
            "report": s.text(0),
            "line": s.int(1),
            "pos": s.int(2),
            "text": s.text(3),
            "inline_code": s.int(4).unwrap_or(0) != 0,
            "section_kind": s.text(5),
            "attributes": s.text(6).and_then(|t| serde_json::from_str::<Value>(t).ok()),
        })).collect::<Vec<_>>(),
    }))
}

/// `claims.history`: the ref's events in chain order (`claim_events`).
///
/// # Errors
/// `E6201` unknown ref; `Error::Index`.
pub fn claims_history(core: &mut Core, claim_ref: ClaimRef) -> Result<Value> {
    let mut tx = core.index.begin()?;
    if claim_row(&mut tx, claim_ref)?.is_none() {
        return Err(Error::code(Code::E6201, format!("no claim {claim_ref}")));
    }
    let rows = tx.query_rows(
        "SELECT ce.event_id, ce.type, ce.at, e.seq FROM claim_events ce LEFT JOIN events e ON e.id = ce.event_id WHERE ce.ref = ? ORDER BY e.seq",
        &[Param::from(claim_ref.to_string())],
    )?;
    tx.rollback()?;
    Ok(list(rows.iter().map(
        |r| json!({"event": r.text(0), "type": r.text(1), "at": r.text(2), "seq": r.int(3)}),
    )))
}

/// `claims.citing_rows`: the claims whose key map holds the row (11.17);
/// a claim whose final table comes from a `group` is left out.
///
/// # Errors
/// `Error::Index`.
pub fn claims_citing_rows(
    core: &mut Core,
    version: &VersionHandle,
    keys: &[Vec<TypedValue>],
) -> Result<Value> {
    let mut tx = core.index.begin()?;
    let mut out = Vec::new();
    for key in keys {
        let tuple = tm_formats::canonical::canonical(&Value::Array(
            key.iter().map(TypedValue::to_json).collect(),
        ))?;
        let rows = tx.query_rows(
            "SELECT ref FROM claim_keys WHERE version = ? AND key_tuple = ? ORDER BY CAST(substr(ref, 2) AS INTEGER)",
            &[Param::from(version.to_string()), Param::from(tuple.as_str())],
        )?;
        let mut refs = Vec::new();
        for r in rows {
            let Some(cr) = r.text(0).and_then(ClaimRef::parse) else {
                continue;
            };
            let Some(row) = claim_row(&mut tx, cr)? else {
                continue;
            };
            let content = revision_content(&mut tx, &row.current_revision)?;
            if has_group(&content) {
                continue;
            }
            refs.push(json!({
                "ref": cr.to_string(),
                "state": row.state.as_str(),
                "sentence": content.sentence,
            }));
        }
        out.push(
            json!({"key": key.iter().map(TypedValue::to_json).collect::<Vec<_>>(), "claims": refs}),
        );
    }
    tx.rollback()?;
    Ok(Value::Array(out))
}

/// True when the content's steps hold a `group` op (11.17's `citing_rows`
/// rule).
#[must_use]
pub fn has_group(content: &Content) -> bool {
    content
        .steps
        .as_ref()
        .and_then(|s| s.get("ops"))
        .and_then(Value::as_array)
        .is_some_and(|ops| {
            ops.iter()
                .any(|op| op.get("op").and_then(Value::as_str) == Some("group"))
        })
}

/// `claims.evidence`: step 0 and every step's table capped at `limit` with
/// its true row count and description, the emphasized rows and columns,
/// the rows beyond the cap, and the assertion result, recomputed from the
/// parse cache (section 10; 11.20).
///
/// # Errors
/// `E6201` unknown ref; the Checker run's codes; `Error::Index`.
pub fn claims_evidence(
    core: &mut Core,
    claim_ref: ClaimRef,
    limit: Option<usize>,
) -> Result<Value> {
    let (row, content) = {
        let mut tx = core.index.begin()?;
        let row = claim_row(&mut tx, claim_ref)?
            .ok_or_else(|| Error::code(Code::E6201, format!("no claim {claim_ref}")))?;
        let content = revision_content(&mut tx, &row.current_revision)?;
        tx.rollback()?;
        (row, content)
    };
    let limit = limit.unwrap_or_else(|| core.settings.rows_per_page());
    let run = if row.kind.has_assertion_check() {
        crate::ops::claim_check::checker_run(core, &content, limit)?
    } else {
        None
    };
    let Some(run) = run else {
        return Ok(json!({
            "ref": claim_ref.to_string(),
            "revision": row.current_revision,
            "steps": Vec::<Value>::new(),
            "emphasized_rows": Vec::<Value>::new(),
            "beyond_cap": Vec::<Value>::new(),
            "emphasized_columns": Vec::<String>::new(),
            "assertion": Value::Null,
            "verdict": Value::Null,
            "limit": limit,
        }));
    };
    let rows_json = |rows: &[Vec<TypedValue>]| -> Vec<Value> {
        rows.iter()
            .map(|r| Value::Array(r.iter().map(TypedValue::to_json).collect()))
            .collect()
    };
    let steps: Vec<Value> = run
        .steps
        .iter()
        .enumerate()
        .map(|(i, s)| {
            json!({
                "step": i,
                "op": s.op,
                "description": s.description,
                "columns": s.columns.iter().map(|(n, t)| json!({"name": n, "type": t.as_str()})).collect::<Vec<_>>(),
                "key": s.key,
                "rows": rows_json(&s.rows),
                "row_count": s.row_count,
            })
        })
        .collect();
    let role = |r: tm_checker::Role| match r {
        tm_checker::Role::Row => "row",
        tm_checker::Role::Left => "left",
        tm_checker::Role::Right => "right",
    };
    Ok(json!({
        "ref": claim_ref.to_string(),
        "revision": row.current_revision,
        "steps": steps,
        "emphasized_rows": run.emphasized_rows.iter().map(|r| json!({
            "role": role(r.role),
            "key": r.key.iter().map(TypedValue::to_json).collect::<Vec<_>>(),
            "position": r.position,
        })).collect::<Vec<_>>(),
        "beyond_cap": run.beyond_cap.iter().map(|r| json!({
            "role": role(r.role),
            "position": r.position,
            "of": r.of,
            "row": r.row.iter().map(TypedValue::to_json).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "emphasized_columns": run.emphasized_columns,
        "assertion": run.verdict.assertion.as_ref().map(|a| json!({
            "outcome": a.outcome.as_str(), "observed": a.observed, "expected": a.expected,
        })),
        "verdict": run.verdict.outcome.as_str(),
        "error": run.verdict.error.as_ref().map(|e| json!({"code": e.code.as_str(), "field": e.field, "message": e.message})),
        "limit": limit,
    }))
}

/// A `claim.added` body's type, for callers that check what they read.
#[must_use]
pub fn is_claim_added(k: &KnownEvent) -> bool {
    k.ty() == EventType::ClaimAdded
}
