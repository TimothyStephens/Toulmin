//! The Claims fold (item 70; rule 6; 11.17): `report.registered` and the
//! `claim.` events into `reports`, `claims`, `claim_revisions`,
//! `claim_cites`, `claim_keys`, `claim_events`, `verdicts`, `reviews`, and
//! the event-derived rows of `flags`, through an [`IndexTx`]. It never
//! writes `spans` (scan-derived) or another library's tables, never reads
//! the record directory or the working tree, and never reads an earlier
//! revision: every revision restates the claim in full, and `against`
//! names the `key_map` version first (11.16).
//!
//! A fold first looks the event ID up in `events` and returns when present
//! (the idempotence check of 11.17); the Core writes `events` after every
//! fold ran.

use serde_json::Value;
use tm_formats::canonical::canonical;
use tm_formats::envelope::Line;
use tm_formats::events::{EventType, KnownEvent};
use tm_index::{IndexTx, Param};

use crate::Result;
use crate::claim::State;
use crate::flags::{Name, REASON_NOT_REVIEWED, SUBJECT_CLAIM, assertion_fails_reason};

fn text(body: &KnownEvent, key: &str) -> String {
    body.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn opt_text(body: &KnownEvent, key: &str) -> Option<String> {
    body.get(key).and_then(Value::as_str).map(str::to_owned)
}

/// Canonical JSON of a member, `NULL` for an absent or `null` member.
fn opt_json(body: &KnownEvent, key: &str) -> Result<Option<String>> {
    match body.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => Ok(Some(canonical(v)?)),
    }
}

fn json_of(body: &KnownEvent, key: &str) -> Result<String> {
    Ok(canonical(body.get(key).unwrap_or(&Value::Null))?)
}

struct Ctx<'b> {
    id: &'b str,
    at: &'b str,
    ty: EventType,
    body: KnownEvent,
}

/// Fold one main-chain line. Lines of other libraries' types and opaque
/// lines are ignored.
///
/// # Errors
/// `Error::Index` when SQLite refuses; `E1002` when a `claim.` or
/// `report.` line's body does not validate (a chain the writer validated
/// never produces this).
pub fn fold(tx: &mut IndexTx<'_>, line: &Line) -> Result<()> {
    let Some(ty) = line
        .envelope
        .get("type")
        .and_then(Value::as_str)
        .and_then(EventType::from_name)
    else {
        return Ok(());
    };
    if !(ty.name().starts_with("claim.") || ty.name().starts_with("report.")) {
        return Ok(());
    }
    let id = line.id.as_str();
    if !tx
        .query_rows("SELECT 1 FROM events WHERE id = ?", &[Param::from(id)])?
        .is_empty()
    {
        return Ok(());
    }
    let tv = line.envelope.get("tv").and_then(Value::as_u64).unwrap_or(1);
    let body = KnownEvent::new_at(ty, tv, line.envelope.get("body").unwrap_or(&Value::Null))?;
    let at = line
        .envelope
        .get("at")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let c = Ctx { id, at, ty, body };
    match ty {
        EventType::ReportRegistered => {
            return exec(
                tx,
                "INSERT OR IGNORE INTO reports (report, path, event_id) VALUES (?, ?, ?)",
                &[
                    text(&c.body, "report").into(),
                    text(&c.body, "path").into(),
                    id.into(),
                ],
            );
        }
        EventType::ClaimAdded => added(tx, &c)?,
        EventType::ClaimRevised => revised(tx, &c)?,
        EventType::ClaimChecked => checked(tx, &c)?,
        EventType::ClaimReviewed => reviewed(tx, &c)?,
        EventType::ClaimRetracted => {
            transition(tx, &c, State::after_retract, None)?;
            clear(tx, &c, Name::AssertionFails)?;
            clear(tx, &c, Name::PromotedNotReviewed)?;
        }
        EventType::ClaimSuperseded => {
            transition(
                tx,
                &c,
                State::after_supersede,
                Some(text(&c.body, "replacement")),
            )?;
            clear(tx, &c, Name::AssertionFails)?;
            clear(tx, &c, Name::PromotedNotReviewed)?;
        }
        _ => return Ok(()),
    }
    exec(
        tx,
        "INSERT OR IGNORE INTO claim_events (ref, event_id, type, at) VALUES (?, ?, ?, ?)",
        &[
            text(&c.body, "ref").into(),
            id.into(),
            c.ty.name().into(),
            at.into(),
        ],
    )
}

fn exec(tx: &mut IndexTx<'_>, sql: &str, params: &[Param]) -> Result<()> {
    tx.execute(sql, params)?;
    Ok(())
}

fn state_of(tx: &mut IndexTx<'_>, claim_ref: &str) -> Result<Option<State>> {
    Ok(tx
        .query_rows(
            "SELECT state FROM claims WHERE ref = ?",
            &[claim_ref.into()],
        )?
        .first()
        .and_then(|r| r.text(0).and_then(State::parse)))
}

fn raise(tx: &mut IndexTx<'_>, c: &Ctx<'_>, name: Name, reason: &str) -> Result<()> {
    exec(
        tx,
        "INSERT OR REPLACE INTO flags (subject_kind, subject, name, reason, since_event) \
         VALUES (?, ?, ?, ?, ?)",
        &[
            SUBJECT_CLAIM.into(),
            text(&c.body, "ref").into(),
            name.as_str().into(),
            reason.into(),
            c.id.into(),
        ],
    )
}

fn clear(tx: &mut IndexTx<'_>, c: &Ctx<'_>, name: Name) -> Result<()> {
    exec(
        tx,
        "DELETE FROM flags WHERE subject_kind = ? AND subject = ? AND name = ?",
        &[
            SUBJECT_CLAIM.into(),
            text(&c.body, "ref").into(),
            name.as_str().into(),
        ],
    )
}

/// A `claim_revisions` row and the ref's `claim_cites`, replaced.
fn revision(tx: &mut IndexTx<'_>, c: &Ctx<'_>) -> Result<()> {
    let claim_ref = text(&c.body, "ref");
    exec(
        tx,
        "INSERT OR IGNORE INTO claim_revisions (event_id, ref, sentence, cited, citations, steps, assertion, qualifiers, at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        &[
            c.id.into(),
            claim_ref.as_str().into(),
            text(&c.body, "sentence").into(),
            json_of(&c.body, "cited")?.into(),
            json_of(&c.body, "citations")?.into(),
            opt_json(&c.body, "steps")?.into(),
            opt_json(&c.body, "assertion")?.into(),
            opt_text(&c.body, "qualifiers").into(),
            c.at.into(),
        ],
    )?;
    exec(
        tx,
        "DELETE FROM claim_cites WHERE ref = ?",
        &[claim_ref.as_str().into()],
    )?;
    for v in c
        .body
        .get("cited")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
    {
        exec(
            tx,
            "INSERT OR IGNORE INTO claim_cites (ref, version) VALUES (?, ?)",
            &[
                claim_ref.as_str().into(),
                v.as_str().unwrap_or_default().into(),
            ],
        )?;
    }
    Ok(())
}

fn added(tx: &mut IndexTx<'_>, c: &Ctx<'_>) -> Result<()> {
    exec(
        tx,
        "INSERT OR IGNORE INTO claims (ref, id, kind, state, created_event, current_revision, report, supersedes, superseded_by) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, NULL)",
        &[
            text(&c.body, "ref").into(),
            c.id.into(),
            text(&c.body, "kind").into(),
            State::Added.as_str().into(),
            c.id.into(),
            c.id.into(),
            text(&c.body, "report").into(),
            opt_text(&c.body, "supersedes").into(),
        ],
    )?;
    revision(tx, c)?;
    raise(tx, c, Name::PromotedNotReviewed, REASON_NOT_REVIEWED)
}

fn revised(tx: &mut IndexTx<'_>, c: &Ctx<'_>) -> Result<()> {
    revision(tx, c)?;
    exec(
        tx,
        "UPDATE claims SET current_revision = ? WHERE ref = ?",
        &[c.id.into(), text(&c.body, "ref").into()],
    )?;
    clear(tx, c, Name::AssertionFails)
}

fn checked(tx: &mut IndexTx<'_>, c: &Ctx<'_>) -> Result<()> {
    let claim_ref = text(&c.body, "ref");
    let against: Vec<String> = c
        .body
        .get("against")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    let evidence = c
        .body
        .get("evidence")
        .and_then(|e| e.get("outcome"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let assertion = c
        .body
        .get("assertion")
        .and_then(|a| a.get("outcome"))
        .and_then(Value::as_str);
    exec(
        tx,
        "INSERT OR IGNORE INTO verdicts (event_id, ref, revision, against, evidence_outcome, assertion_outcome, body, at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        &[
            c.id.into(),
            claim_ref.as_str().into(),
            text(&c.body, "revision").into(),
            json_of(&c.body, "against")?.into(),
            evidence.into(),
            assertion.into(),
            canonical(&Value::Object(c.body.body().clone()))?.into(),
            c.at.into(),
        ],
    )?;
    if let Some(version) = against.first() {
        exec(
            tx,
            "DELETE FROM claim_keys WHERE ref = ? AND version = ?",
            &[claim_ref.as_str().into(), version.as_str().into()],
        )?;
        for tuple in c
            .body
            .get("key_map")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default()
        {
            exec(
                tx,
                "INSERT OR IGNORE INTO claim_keys (ref, version, key_tuple) VALUES (?, ?, ?)",
                &[
                    claim_ref.as_str().into(),
                    version.as_str().into(),
                    canonical(tuple)?.into(),
                ],
            )?;
        }
    }
    let live = state_of(tx, &claim_ref)?.is_some_and(|s| !s.is_final());
    if assertion == Some("fails") && live {
        raise(
            tx,
            c,
            Name::AssertionFails,
            &assertion_fails_reason(&against),
        )?;
    }
    Ok(())
}

fn reviewed(tx: &mut IndexTx<'_>, c: &Ctx<'_>) -> Result<()> {
    exec(
        tx,
        "INSERT OR IGNORE INTO reviews (event_id, ref, revision, outcome, note, at) VALUES (?, ?, ?, ?, ?, ?)",
        &[
            c.id.into(),
            text(&c.body, "ref").into(),
            text(&c.body, "revision").into(),
            text(&c.body, "outcome").into(),
            opt_text(&c.body, "note").into(),
            c.at.into(),
        ],
    )?;
    transition(tx, c, State::after_review, None)?;
    clear(tx, c, Name::PromotedNotReviewed)
}

fn transition(
    tx: &mut IndexTx<'_>,
    c: &Ctx<'_>,
    next: fn(State) -> State,
    superseded_by: Option<String>,
) -> Result<()> {
    let claim_ref = text(&c.body, "ref");
    let Some(state) = state_of(tx, &claim_ref)? else {
        return Ok(());
    };
    let after = next(state);
    if after == state {
        return Ok(());
    }
    match superseded_by {
        Some(by) => exec(
            tx,
            "UPDATE claims SET state = ?, superseded_by = ? WHERE ref = ?",
            &[after.as_str().into(), by.into(), claim_ref.into()],
        ),
        None => exec(
            tx,
            "UPDATE claims SET state = ? WHERE ref = ?",
            &[after.as_str().into(), claim_ref.into()],
        ),
    }
}
