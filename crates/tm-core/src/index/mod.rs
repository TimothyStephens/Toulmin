//! The index (item 76; 3.4; 11.17): one SQLite file per machine and
//! project, rebuildable from the main chain, never the source of truth.
//!
//! `mod.rs` opens the database, creates the 11.17 schema, and hands
//! `tm-index`'s `IndexTx` to the folds; `apply.rs` folds events (each
//! library's fold writes its own tables, the Core writes `head`, `events`,
//! `opaque_events`, `settings`, and `audits`); `locate.rs` resolves the
//! path (3.4), runs `PRAGMA integrity_check`, and decides when a rebuild
//! is due. A full rebuild drops every table except `stat_cache` and
//! `spans` and replays from the first segment.

pub mod apply;
pub mod locate;

use std::path::Path;

use tm_index::{Index, IndexTx, Param};
use tm_record::{Position, Record, StoredEvent};

use crate::{Code, Error, Result};

/// The 11.17 `schema.sql` text, byte for byte the design's code block.
pub const SCHEMA_SQL: &str = include_str!("schema.sql");

/// The stat cache table (3.4; 11.17's prose), in the same database and
/// never dropped.
pub const STAT_CACHE_SQL: &str = "CREATE TABLE IF NOT EXISTS stat_cache (path TEXT PRIMARY KEY, \
    snapshot TEXT, fingerprint TEXT, confirmed_at TEXT);";

/// The Core's index schema version (`head.schema_version`, 11.17).
pub const SCHEMA_VERSION: u64 = 1;

/// The tables a rebuild keeps (11.17).
pub const KEPT_ON_REBUILD: &[&str] = &["stat_cache", "spans"];

/// Every table 11.17 creates, in schema order.
#[must_use]
pub fn tables() -> Vec<String> {
    SCHEMA_SQL
        .split(';')
        .filter_map(|stmt| {
            let s = stmt.trim_start();
            s.strip_prefix("CREATE TABLE ")
                .and_then(|r| r.split_whitespace().next())
                .map(str::to_owned)
        })
        .collect()
}

/// `CREATE TABLE IF NOT EXISTS` for every statement of the schema, so the
/// kept tables survive a rebuild and a first open creates everything.
fn schema_if_not_exists() -> String {
    let mut out = String::new();
    for stmt in SCHEMA_SQL.split(';') {
        let s = stmt.trim();
        if s.is_empty() {
            continue;
        }
        let s = s
            .strip_prefix("CREATE TABLE ")
            .map(|r| format!("CREATE TABLE IF NOT EXISTS {r}"))
            .or_else(|| {
                s.strip_prefix("CREATE INDEX ")
                    .map(|r| format!("CREATE INDEX IF NOT EXISTS {r}"))
            })
            .unwrap_or_else(|| s.to_owned());
        out.push_str(&s);
        out.push_str(";\n");
    }
    out.push_str(STAT_CACHE_SQL);
    out.push('\n');
    out
}

/// Open (or create) the index at `path`, create any missing table, and
/// check its integrity; a database that fails the check is removed and
/// recreated empty, which the next [`sync`] fills (3.4).
///
/// # Errors
/// `Error::Index`; `Error::Io` when the directory cannot be created.
pub fn open(path: &Path) -> Result<Index> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }
    let index = Index::open(path)?;
    let ok = index.integrity_ok().unwrap_or(false);
    let index = if ok {
        index
    } else {
        drop(index);
        let _ = std::fs::remove_file(path);
        for suffix in ["-wal", "-shm", "-journal"] {
            let mut side = path.as_os_str().to_owned();
            side.push(suffix);
            let _ = std::fs::remove_file(side);
        }
        Index::open(path)?
    };
    index.execute_batch(&schema_if_not_exists())?;
    Ok(index)
}

/// An in-memory index with the schema (tests and read-only commands with
/// no cache directory).
///
/// # Errors
/// `Error::Index`.
pub fn in_memory() -> Result<Index> {
    let index = Index::in_memory()?;
    index.execute_batch(&schema_if_not_exists())?;
    Ok(index)
}

/// What [`sync`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sync {
    /// The index was already at the chain's head.
    Current,
    /// New batches were folded.
    CaughtUp {
        /// Events folded.
        events: u64,
    },
    /// The index was rebuilt from the first segment.
    Rebuilt {
        /// Events folded.
        events: u64,
    },
}

/// Bring the index to the record's head: fold the batches after the
/// recorded head, or rebuild when the recorded head is not on the chain,
/// is ahead of it, or carries another schema version (3.4).
///
/// # Errors
/// `Error::Index`; `Error::Record` for a chain that does not read.
pub fn sync(index: &mut Index, record: &Record) -> Result<Sync> {
    let head = {
        let mut tx = index.begin()?;
        let h = tx.head("main")?;
        tx.rollback()?;
        h
    };
    let record_head = record.head()?;
    let Some(h) = head else {
        return Ok(Sync::Rebuilt {
            events: rebuild(index, record)?,
        });
    };
    if h.schema_version != SCHEMA_VERSION || h.seq > record_head.seq {
        return Ok(Sync::Rebuilt {
            events: rebuild(index, record)?,
        });
    }
    if h.seq == 0 {
        // Nothing folded yet, nothing to check.
        let n = fold_after(index, record, None)?;
        return Ok(if n == 0 {
            Sync::Current
        } else {
            Sync::CaughtUp { events: n }
        });
    }
    let position = Position {
        segment: h.segment.unwrap_or(1),
        offset: h.offset.unwrap_or(0),
    };
    let on_chain = record
        .read_at(position)
        .is_ok_and(|e| e.id().as_str() == h.last_id);
    if !on_chain {
        return Ok(Sync::Rebuilt {
            events: rebuild(index, record)?,
        });
    }
    if h.seq == record_head.seq {
        return Ok(Sync::Current);
    }
    Ok(Sync::CaughtUp {
        events: fold_after(index, record, Some(position))?,
    })
}

/// Drop every rebuildable table and replay the chain from the first
/// segment (3.4; 11.17). Returns the number of events folded.
///
/// # Errors
/// `Error::Index`; `Error::Record`.
pub fn rebuild(index: &mut Index, record: &Record) -> Result<u64> {
    let mut drops = String::new();
    for t in tables() {
        if !KEPT_ON_REBUILD.contains(&t.as_str()) {
            drops.push_str(&format!("DROP TABLE IF EXISTS {t};\n"));
        }
    }
    index.execute_batch(&drops)?;
    index.execute_batch(&schema_if_not_exists())?;
    fold_after(index, record, None)
}

/// Fold every committed event after `after` (all of them for `None`),
/// one transaction per batch.
fn fold_after(index: &mut Index, record: &Record, after: Option<Position>) -> Result<u64> {
    let scan = match after {
        Some(p) => record.events_after(p)?,
        None => record.events()?,
    };
    let mut folded = 0u64;
    let mut batch: Vec<StoredEvent> = Vec::new();
    for e in scan {
        let e = e?;
        if batch.first().is_some_and(|f| f.batch != e.batch) {
            folded += apply_batch(index, &batch)?;
            batch.clear();
        }
        batch.push(e);
    }
    if !batch.is_empty() {
        folded += apply_batch(index, &batch)?;
    }
    Ok(folded)
}

/// Fold one main-chain batch in one transaction (the 11.17 fold contract)
/// and move `head` to its last event.
///
/// # Errors
/// `Error::Index`; a fold's `E1002` on a line whose body does not
/// validate.
pub fn apply_batch(index: &mut Index, batch: &[StoredEvent]) -> Result<u64> {
    let Some(last) = batch.last() else {
        return Ok(0);
    };
    let mut tx = index.begin()?;
    let mut n = 0u64;
    for e in batch {
        if apply::fold_event(&mut tx, e)? {
            n += 1;
        }
    }
    write_head(&mut tx, last)?;
    tx.commit()?;
    Ok(n)
}

fn write_head(tx: &mut IndexTx<'_>, last: &StoredEvent) -> Result<()> {
    tx.execute(
        "INSERT OR REPLACE INTO head (chain, last_id, seq, segment, offset, batch, epoch, schema_version) \
         VALUES ('main', ?, ?, ?, ?, ?, ?, ?)",
        &[
            Param::from(last.id().as_str()),
            Param::from(last.seq),
            Param::from(last.position.segment),
            Param::from(last.position.offset),
            Param::from(last.batch),
            Param::from(last.epoch),
            Param::from(SCHEMA_VERSION),
        ],
    )?;
    Ok(())
}

/// Resolve an event ID to its position through the `events` table and
/// read it from the record (section 9's resolve-by-ID rule, item 37).
///
/// # Errors
/// `E2103` when the ID is not in the index, or the position does not hold
/// it; `Error::Index`.
pub fn read_event(tx: &mut IndexTx<'_>, record: &Record, id: &str) -> Result<StoredEvent> {
    let rows = tx.query_rows(
        "SELECT segment, offset FROM events WHERE id = ?",
        &[Param::from(id)],
    )?;
    let Some(row) = rows.first() else {
        return Err(Error::code(
            Code::E2103,
            format!("event {id} is not in the index; it is not on the chain"),
        ));
    };
    let position = Position {
        segment: row.get(0).as_u64().unwrap_or(1),
        offset: row.get(1).as_u64().unwrap_or(0),
    };
    let e = record.read_at(position)?;
    if e.id().as_str() != id {
        return Err(Error::code(
            Code::E2103,
            format!(
                "the index places {id} at segment {} offset {}, which holds {}",
                position.segment,
                position.offset,
                e.id()
            ),
        ));
    }
    Ok(e)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_is_the_design_block() {
        let design = std::fs::read_to_string(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../docs/design/toulmin_v1_design.md"),
        )
        .unwrap();
        let start = design.find("#### 11.17 Index schema").unwrap();
        let rest = &design[start..];
        let sql = &rest[rest.find("```sql\n").unwrap() + 7..];
        assert_eq!(&sql[..sql.find("```").unwrap()], SCHEMA_SQL);
        let t = tables();
        assert!(t.contains(&"events".to_owned()) && t.contains(&"spans".to_owned()));
        let index = in_memory().unwrap();
        assert!(index.integrity_ok().unwrap());
    }
}
