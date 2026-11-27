//! Folding one event (item 76; rule 6; 11.17's fold contract).
//!
//! The Core runs each library's fold, which writes only that library's
//! tables (`tm-files`: `files`, `file_versions`, `parse_specs`, `panels`;
//! `tm-claims`: `reports`, `claims`, `claim_revisions`, `claim_cites`,
//! `claim_keys`, `claim_events`, `verdicts`, `reviews`, the event-derived
//! `flags`), then writes its own: `settings` from `settings.changed`,
//! `audits` from `audit.completed`, `opaque_events` for a line it does not
//! interpret (11.11), and the `events` row every fold's idempotence check
//! looks up, which is why it is written last.

use serde_json::Value;
use tm_formats::canonical::canonical;
use tm_formats::events::{Event, EventType, KnownEvent};
use tm_index::{IndexTx, Param};
use tm_record::StoredEvent;

use crate::Result;
use crate::settings::fold_settings_changed;

/// Fold one stored event; `Ok(false)` when it was already folded.
///
/// # Errors
/// `Error::Index`; a library fold's `E1002`.
pub fn fold_event(tx: &mut IndexTx<'_>, e: &StoredEvent) -> Result<bool> {
    let id = e.id().as_str();
    if !tx
        .query_rows("SELECT 1 FROM events WHERE id = ?", &[Param::from(id)])?
        .is_empty()
    {
        return Ok(false);
    }
    match &e.line.event {
        Event::Known(k) => {
            tm_files::fold::fold(tx, &e.line)?;
            tm_claims::fold::fold(tx, &e.line)?;
            core_fold(tx, id, e, k)?;
        }
        Event::Opaque(o) => {
            tx.execute(
                "INSERT OR IGNORE INTO opaque_events (id, type, tv) VALUES (?, ?, ?)",
                &[
                    Param::from(id),
                    Param::from(o.ty.as_str()),
                    Param::from(o.tv),
                ],
            )?;
        }
    }
    tx.execute(
        "INSERT OR IGNORE INTO events (id, chain, seq, type, segment, offset, at) VALUES (?, ?, ?, ?, ?, ?, ?)",
        &[
            Param::from(id),
            Param::from(
                e.line
                    .envelope
                    .get("chain")
                    .and_then(Value::as_str)
                    .unwrap_or("main"),
            ),
            Param::from(e.seq),
            Param::from(e.type_name()),
            Param::from(e.position.segment),
            Param::from(e.position.offset),
            Param::from(at(e)),
        ],
    )?;
    Ok(true)
}

fn at(e: &StoredEvent) -> &str {
    e.line
        .envelope
        .get("at")
        .and_then(Value::as_str)
        .unwrap_or_default()
}

/// The Core-owned tables (11.17).
fn core_fold(tx: &mut IndexTx<'_>, id: &str, e: &StoredEvent, k: &KnownEvent) -> Result<()> {
    match k.ty() {
        EventType::SettingsChanged => fold_settings_changed(tx, id, k.body()),
        EventType::AuditCompleted => {
            let s = |key: &str| k.get(key).and_then(Value::as_str).map(str::to_owned);
            tx.execute(
                "INSERT OR IGNORE INTO audits (event_id, at, since, report_hash, counts) VALUES (?, ?, ?, ?, ?)",
                &[
                    Param::from(id),
                    Param::from(at(e)),
                    Param::from(s("since")),
                    Param::from(s("report_hash")),
                    Param::from(canonical(k.get("counts").unwrap_or(&Value::Null))?),
                ],
            )?;
            Ok(())
        }
        _ => Ok(()),
    }
}
