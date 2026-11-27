//! `audit.last` (section 10; item 78): the last audit's `counts`, `since`,
//! `report_hash`, and the report's path relative to an export directory.

use serde_json::{Value, json};

use crate::Result;
use crate::audit::relative_html_path;
use crate::ops::Core;

/// `audit.last`; `null` when no audit has run.
///
/// # Errors
/// `Error::Index`.
pub fn audit_last(core: &mut Core) -> Result<Value> {
    let mut tx = core.tx()?;
    let rows = tx.query_rows(
        "SELECT event_id, at, since, report_hash, counts FROM audits ORDER BY at DESC, rowid DESC LIMIT 1",
        &[],
    )?;
    tx.rollback()?;
    let Some(r) = rows.first() else {
        return Ok(Value::Null);
    };
    let hash = r.text(3).unwrap_or_default();
    Ok(json!({
        "event": r.text(0),
        "at": r.text(1),
        "since": r.text(2),
        "report_hash": hash,
        "counts": r.text(4).and_then(|c| serde_json::from_str::<Value>(c).ok()),
        "path": relative_html_path(hash),
    }))
}
