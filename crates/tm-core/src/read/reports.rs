//! `reports.list` and `reports.spans` (section 10; item 78).

use serde_json::{Value, json};
use tm_claims::drift::Drift;
use tm_formats::id::ReportHandle;

use crate::ops::report_check::drifts;
use crate::ops::{Core, reports};
use crate::read::list;
use crate::{Code, Error, Result};

/// `reports.list`: registered reports with span and drift counts.
///
/// # Errors
/// `Error::Index`.
pub fn reports_list(core: &mut Core) -> Result<Value> {
    let rows = {
        let mut tx = core.tx()?;
        let r = reports(&mut tx)?;
        tx.rollback()?;
        r
    };
    let all = drifts(core, None)?;
    Ok(list(rows.iter().map(|r| {
        let mine: Vec<_> = all.iter().filter(|d| d.span.report == r.report).collect();
        json!({
            "report": r.report.to_string(),
            "path": r.path,
            "event": r.event_id,
            "spans": mine.len(),
            "drifted": mine.iter().filter(|d| d.drift == Some(Drift::Drifted)).count(),
            "not_checkable": mine.iter().filter(|d| d.drift == Some(Drift::NotCheckable)).count(),
        })
    })))
}

/// `reports.spans`: the spans of one report in file order, with the
/// recorded sentence, the current text, and the drift state.
///
/// # Errors
/// `E6201` unknown report; `Error::Index`.
pub fn reports_spans(core: &mut Core, report: &str) -> Result<Value> {
    let handle = match ReportHandle::parse(report) {
        Some(h) => h,
        None => super::super::ops::report_by_handle_or_path(core, report)?.report,
    };
    {
        let mut tx = core.tx()?;
        let known = reports(&mut tx)?.iter().any(|r| r.report == handle);
        tx.rollback()?;
        if !known {
            return Err(Error::code(Code::E6201, format!("no report {handle}")));
        }
    }
    Ok(list(
        drifts(core, Some(handle))?
            .iter()
            .map(super::super::ops::report_check::SpanDrift::to_json),
    ))
}
