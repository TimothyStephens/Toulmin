//! `report.add` (section 9; 11.16; item 74): register a Markdown report,
//! allocating the next `T<n>` under the lock, then rescan every registered
//! report's spans (11.17). A path already registered writes nothing and
//! returns its handle after rescanning.

use std::path::Path;

use serde_json::json;
use tm_formats::events::{EventType, KnownEvent};
use tm_formats::id::ReportHandle;
use tm_owner_cap::OwnerCap;

use crate::ops::report_check::rescan;
use crate::ops::{Core, next_report_handle, owner, reports};
use crate::{Code, Error, Result};

/// What `report.add` did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportAdded {
    /// The handle.
    pub report: ReportHandle,
    /// The repository-relative path.
    pub path: String,
    /// Newly registered (false when the path was already registered).
    pub registered: bool,
    /// Spans found in it.
    pub spans: usize,
}

/// A Markdown file: its path ends in `.md` or `.markdown` (item 74).
#[must_use]
pub fn is_markdown(rel: &str) -> bool {
    let lower = rel.to_ascii_lowercase();
    lower.ends_with(".md") || lower.ends_with(".markdown")
}

/// `tm report add PATH`.
///
/// # Errors
/// `E6601` not a Markdown file (or not a file in the repository); `E3001`
/// outside the project; the lock's and the append's.
pub fn add(core: &mut Core, cap: &OwnerCap, path: &Path) -> Result<ReportAdded> {
    let rel = core.relative(path)?;
    if !is_markdown(&rel) {
        return Err(Error::code(
            Code::E6601,
            format!("{rel} is not a Markdown file (.md or .markdown)"),
        ));
    }
    if !core.project.abs(&rel).is_file() {
        return Err(Error::code(
            Code::E6601,
            format!("{rel} is not a file in the repository"),
        ));
    }
    let mut locked = core.lock(|_, _| {})?;
    let existing = {
        let mut tx = locked.core.tx()?;
        let r = reports(&mut tx)?.into_iter().find(|r| r.path == rel);
        tx.rollback()?;
        r
    };
    let (handle, registered) = match existing {
        Some(r) => (r.report, false),
        None => {
            let h = {
                let mut tx = locked.core.tx()?;
                let h = next_report_handle(&mut tx)?;
                tx.rollback()?;
                h
            };
            let ev = KnownEvent::new(
                EventType::ReportRegistered,
                &json!({"report": h.to_string(), "path": rel}),
            )?;
            locked.commit(&owner(cap), &[ev])?;
            (h, true)
        }
    };
    let spans = rescan(locked.core)?
        .iter()
        .filter(|s| s.report == handle)
        .count();
    locked.release()?;
    Ok(ReportAdded {
        report: handle,
        path: rel,
        registered,
        spans,
    })
}
