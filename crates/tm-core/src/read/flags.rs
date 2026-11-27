//! `flags.list` and the scan-derived flags (section 10; 5.6; item 66; item
//! 78): every open flag and label in one shape, from three sources. The
//! event-derived flags the Claims fold stores in `flags` (`assertion
//! fails`, `promoted, not reviewed`); the labels read from the rows their
//! events fold into (`retracted`, `superseded`, `evidence not kept`,
//! `removable`, `restored`); and the scan-derived ones computed here from
//! the stat cache, the scanned `spans` table, and the drift check (`text
//! drifted`, `evidence moved`, `expected-missing`, `possibly changed
//! (unconfirmed)`, `points at a retracted claim`, `points at a superseded
//! claim`).

use tm_claims::drift::Drift;
use tm_claims::flags::{Name, NameKind};
use tm_formats::events::Event;
use tm_formats::id::ClaimRef;
use tm_index::IndexTx;

use crate::ops::report_check::drifts;
use crate::ops::{Core, files, latest_version, version_by_handle};
use crate::read::Flag;
use crate::read::claims::{claim_rows, revision_content};
use crate::{Result, index, stat_cache};

fn flag(
    name: Name,
    subject_kind: &str,
    subject: impl Into<String>,
    reason: impl Into<String>,
) -> Flag {
    Flag {
        name: name.as_str().to_owned(),
        label: name.kind() == NameKind::Label,
        subject_kind: subject_kind.to_owned(),
        subject: subject.into(),
        reason: reason.into(),
        next: name.clearing_action().to_owned(),
    }
}

/// The event-derived rows of `flags` (item 70).
fn stored(tx: &mut IndexTx<'_>) -> Result<Vec<Flag>> {
    let rows = tx.query_rows(
        "SELECT subject_kind, subject, name, reason FROM flags ORDER BY subject, name",
        &[],
    )?;
    Ok(rows
        .iter()
        .filter_map(|r| {
            let name = Name::parse(r.text(2)?)?;
            Some(flag(
                name,
                r.text(0)?,
                r.text(1)?.to_owned(),
                r.text(3)?.to_owned(),
            ))
        })
        .collect())
}

/// The reason text of a `claim.retracted` (its body's `reason`), read by
/// ID through the index.
fn retract_reason(core: &mut Core, claim_ref: ClaimRef) -> Result<Option<String>> {
    let mut tx = core.index.begin()?;
    let rows = tx.query_rows(
        "SELECT event_id FROM claim_events WHERE ref = ? AND type = 'claim.retracted' ORDER BY at DESC LIMIT 1",
        &[tm_index::Param::from(claim_ref.to_string())],
    )?;
    let Some(id) = rows.first().and_then(|r| r.text(0)).map(str::to_owned) else {
        tx.rollback()?;
        return Ok(None);
    };
    let e = index::read_event(&mut tx, &core.project.record, &id)?;
    tx.rollback()?;
    Ok(match &e.line.event {
        Event::Known(k) => k
            .get("reason")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned),
        Event::Opaque(_) => None,
    })
}

/// Every open flag and label (section 10's `flags.list` with no filter).
///
/// # Errors
/// `Error::Index`; `Error::Record`.
pub fn compute(core: &mut Core) -> Result<Vec<Flag>> {
    let mut out = Vec::new();
    // 1. Event-derived flags from the Claims fold.
    {
        let mut tx = core.index.begin()?;
        out.extend(stored(&mut tx)?);
        tx.rollback()?;
    }
    // 2. Labels from rows.
    let claims = {
        let mut tx = core.index.begin()?;
        let c = claim_rows(&mut tx)?;
        tx.rollback()?;
        c
    };
    let mut retract_reasons = std::collections::BTreeMap::new();
    for row in &claims {
        let subject = row.claim_ref.to_string();
        match row.state {
            tm_claims::State::Retracted => {
                let reason = retract_reason(core, row.claim_ref)?.unwrap_or_default();
                retract_reasons.insert(row.claim_ref, reason.clone());
                out.push(flag(
                    Name::Retracted,
                    "claim",
                    subject.clone(),
                    format!("Owner retracted it: {reason}"),
                ));
            }
            tm_claims::State::Superseded => {
                let by = row.superseded_by.map(|r| r.to_string()).unwrap_or_default();
                out.push(flag(
                    Name::Superseded,
                    "claim",
                    subject.clone(),
                    format!("A new claim points back at it: replacement {by}"),
                ));
            }
            tm_claims::State::Added | tm_claims::State::Reviewed => {}
        }
    }
    let mut tx = core.index.begin()?;
    // evidence not kept: a cited version kept fingerprint-only.
    for row in &claims {
        let content = revision_content(&mut tx, &row.current_revision)?;
        for v in &content.cited {
            if let Some(vr) = version_by_handle(&mut tx, v)?
                && vr.fingerprint_only
            {
                out.push(flag(
                    Name::EvidenceNotKept,
                    "claim",
                    row.claim_ref.to_string(),
                    format!("Owner kept only the fingerprint of {v}"),
                ));
            }
        }
    }
    let file_rows = files(&mut tx)?;
    for f in &file_rows {
        let subject = f.identity.file.to_string();
        if f.identity.removable {
            out.push(flag(
                Name::Removable,
                "file",
                subject.clone(),
                "Owner marked it",
            ));
        }
        if f.restored {
            out.push(flag(
                Name::Restored,
                "file",
                subject.clone(),
                "Owner restored a retired identity",
            ));
        }
    }
    // 3. Scan-derived: the stat cache against the pinned versions.
    for f in &file_rows {
        let Some(latest) = latest_version(&mut tx, f.identity.file)? else {
            continue;
        };
        let Some(entry) = stat_cache::get(&mut tx, &f.identity.path)? else {
            continue;
        };
        let version = latest.handle.to_string();
        match (&entry.snapshot, &entry.fingerprint) {
            (None, _) => {
                if f.identity.removable && !latest.bytes_kept() {
                    out.push(flag(
                        Name::ExpectedMissing,
                        "version",
                        version.clone(),
                        "Marked removable and no longer on disk",
                    ));
                }
            }
            (Some(_), None) => out.push(flag(
                Name::PossiblyChangedUnconfirmed,
                "version",
                version.clone(),
                format!(
                    "Stat changed on {} (above hashing.inline_threshold) during a disk scan",
                    f.identity.path
                ),
            )),
            (Some(_), Some(_)) => {}
        }
    }
    // evidence moved, on live claims: a cited version whose file on disk
    // differs from the pinned fingerprint, or that has a newer version.
    for row in &claims {
        if row.state.is_final() {
            continue;
        }
        let content = revision_content(&mut tx, &row.current_revision)?;
        for v in &content.cited {
            let Some(f) = super::super::ops::file_by_handle(&mut tx, v.file)? else {
                continue;
            };
            let Some(vr) = version_by_handle(&mut tx, v)? else {
                continue;
            };
            let latest = latest_version(&mut tx, v.file)?;
            if let Some(l) = &latest
                && l.handle.version > v.version
            {
                out.push(flag(
                    Name::EvidenceMoved,
                    "claim",
                    row.claim_ref.to_string(),
                    format!("newer version exists: {} is at {}", v.file, l.handle),
                ));
                continue;
            }
            if let Some(entry) = stat_cache::get(&mut tx, &f.identity.path)?
                && let Some(fp) = &entry.fingerprint
                && entry.snapshot.is_some()
                && *fp != vr.fingerprint
            {
                out.push(flag(
                    Name::EvidenceMoved,
                    "claim",
                    row.claim_ref.to_string(),
                    format!(
                        "disk differs: {} no longer matches {v} (now {fp})",
                        f.identity.path
                    ),
                ));
            }
        }
    }
    tx.rollback()?;
    // text drifted, and the two span flags, from the scanned spans.
    for d in drifts(core, None)? {
        let Some(state) = d.state else {
            continue;
        };
        let span_subject = d.span.subject();
        match state {
            tm_claims::State::Retracted => {
                let reason = retract_reasons
                    .get(&d.span.claim_ref)
                    .cloned()
                    .unwrap_or_default();
                out.push(flag(
                    Name::PointsAtARetractedClaim,
                    "span",
                    span_subject,
                    format!("ref {} is retracted: {reason}", d.span.claim_ref),
                ));
            }
            tm_claims::State::Superseded => {
                let by = claims
                    .iter()
                    .find(|c| c.claim_ref == d.span.claim_ref)
                    .and_then(|c| c.superseded_by)
                    .map(|r| r.to_string())
                    .unwrap_or_default();
                out.push(flag(
                    Name::PointsAtASupersededClaim,
                    "span",
                    span_subject,
                    format!("ref {} is superseded; replacement {by}", d.span.claim_ref),
                ));
            }
            tm_claims::State::Added | tm_claims::State::Reviewed => {
                if d.drift == Some(Drift::Drifted) {
                    out.push(flag(
                        Name::TextDrifted,
                        "claim",
                        d.span.claim_ref.to_string(),
                        format!("the span at {span_subject} differs from the recorded sentence"),
                    ));
                }
            }
        }
    }
    Ok(out)
}

/// `flags.list` with its filters: a 5.6 name and a subject.
///
/// # Errors
/// As [`compute`].
pub fn flags_list(core: &mut Core, kind: Option<&str>, subject: Option<&str>) -> Result<Vec<Flag>> {
    Ok(compute(core)?
        .into_iter()
        .filter(|f| kind.is_none_or(|k| f.name == k))
        .filter(|f| subject.is_none_or(|s| f.subject == s))
        .collect())
}

/// The flags and labels on one claim.
///
/// # Errors
/// As [`compute`].
pub fn flags_of_claim(core: &mut Core, claim_ref: ClaimRef) -> Result<Vec<Flag>> {
    let subject = claim_ref.to_string();
    Ok(compute(core)?
        .into_iter()
        .filter(|f| f.subject_kind == "claim" && f.subject == subject)
        .collect())
}
