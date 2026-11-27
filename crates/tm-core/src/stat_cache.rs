//! The node-local stat cache (item 77; 3.4; 11.17): per tracked path, the
//! last snapshot and the fingerprint it was confirmed against, in the
//! `stat_cache` table of the index database, excluded from rebuild drops.
//! A row whose `snapshot` is `NULL` records that the path was absent at
//! the last scan.

use serde_json::Value;
use tm_formats::id::EventId;
use tm_fsfacts::snapshot::Snapshot;
use tm_index::{IndexTx, Param};

use crate::Result;

/// One row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The repository-relative path.
    pub path: String,
    /// The last snapshot, `None` when the path was absent.
    pub snapshot: Option<Snapshot>,
    /// The fingerprint the snapshot was confirmed against, when it was.
    pub fingerprint: Option<EventId>,
    /// When it was confirmed (11.2's `at` form).
    pub confirmed_at: Option<String>,
}

/// The row for `path`.
///
/// # Errors
/// `Error::Index`.
pub fn get(tx: &mut IndexTx<'_>, path: &str) -> Result<Option<Entry>> {
    let rows = tx.query_rows(
        "SELECT path, snapshot, fingerprint, confirmed_at FROM stat_cache WHERE path = ?",
        &[Param::from(path)],
    )?;
    Ok(rows.first().map(|r| Entry {
        path: r.text(0).unwrap_or_default().to_owned(),
        snapshot: r
            .text(1)
            .and_then(|t| serde_json::from_str::<Value>(t).ok())
            .and_then(|v| Snapshot::from_json(&v)),
        fingerprint: r.text(2).and_then(EventId::parse),
        confirmed_at: r.text(3).map(str::to_owned),
    }))
}

/// Every row.
///
/// # Errors
/// `Error::Index`.
pub fn all(tx: &mut IndexTx<'_>) -> Result<Vec<Entry>> {
    let rows = tx.query_rows(
        "SELECT path, snapshot, fingerprint, confirmed_at FROM stat_cache ORDER BY path",
        &[],
    )?;
    Ok(rows
        .iter()
        .map(|r| Entry {
            path: r.text(0).unwrap_or_default().to_owned(),
            snapshot: r
                .text(1)
                .and_then(|t| serde_json::from_str::<Value>(t).ok())
                .and_then(|v| Snapshot::from_json(&v)),
            fingerprint: r.text(2).and_then(EventId::parse),
            confirmed_at: r.text(3).map(str::to_owned),
        })
        .collect())
}

/// Write a row: the snapshot found (or `None` for an absent path) and the
/// fingerprint it was confirmed against.
///
/// # Errors
/// `Error::Index`.
pub fn put(
    tx: &mut IndexTx<'_>,
    path: &str,
    snapshot: Option<Snapshot>,
    fingerprint: Option<&EventId>,
    confirmed_at: &str,
) -> Result<()> {
    let snap = match snapshot {
        Some(s) => Some(tm_formats::canonical::canonical(&s.to_json())?),
        None => None,
    };
    tx.execute(
        "INSERT OR REPLACE INTO stat_cache (path, snapshot, fingerprint, confirmed_at) VALUES (?, ?, ?, ?)",
        &[
            Param::from(path),
            Param::from(snap),
            Param::from(fingerprint.map(EventId::as_str)),
            Param::from(confirmed_at),
        ],
    )?;
    Ok(())
}

/// Remove a path's row (a rename).
///
/// # Errors
/// `Error::Index`.
pub fn remove(tx: &mut IndexTx<'_>, path: &str) -> Result<()> {
    tx.execute(
        "DELETE FROM stat_cache WHERE path = ?",
        &[Param::from(path)],
    )?;
    Ok(())
}
