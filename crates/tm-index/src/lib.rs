//! Toulmin index transaction (design 4.1, 11.17, item 26b).
//!
//! [`IndexTx`] wraps one `rusqlite::Transaction`. Every library's fold
//! (`fn fold(&mut IndexTx, &Event)`, rule 6) and the Core write the index
//! through it, so no library depends on the Core and the `rusqlite`
//! dependency lives only here. The schema itself (`schema.sql`) is the
//! Core's (item 76); this crate knows only the `head` table's shape.

use std::path::Path;

pub use rusqlite::Error;
use rusqlite::types::{ToSqlOutput, Value, ValueRef};
use rusqlite::{Connection, OpenFlags, ToSql, Transaction};

/// A parameter a fold passes, and a cell a row carries.
#[derive(Debug, Clone, PartialEq)]
pub enum Param {
    /// SQL `NULL`.
    Null,
    /// An integer.
    Int(i64),
    /// Text (canonical JSON for the JSON-valued columns of 11.17).
    Text(String),
    /// Bytes.
    Blob(Vec<u8>),
}

impl From<i64> for Param {
    fn from(v: i64) -> Self {
        Param::Int(v)
    }
}

impl From<u64> for Param {
    fn from(v: u64) -> Self {
        Param::Int(i64::try_from(v).unwrap_or(i64::MAX))
    }
}

impl From<bool> for Param {
    fn from(v: bool) -> Self {
        Param::Int(i64::from(v))
    }
}

impl From<&str> for Param {
    fn from(v: &str) -> Self {
        Param::Text(v.to_owned())
    }
}

impl From<String> for Param {
    fn from(v: String) -> Self {
        Param::Text(v)
    }
}

impl<T: Into<Param>> From<Option<T>> for Param {
    fn from(v: Option<T>) -> Self {
        v.map_or(Param::Null, Into::into)
    }
}

impl ToSql for Param {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(match self {
            Param::Null => ToSqlOutput::Owned(Value::Null),
            Param::Int(i) => ToSqlOutput::Owned(Value::Integer(*i)),
            Param::Text(s) => ToSqlOutput::Borrowed(ValueRef::Text(s.as_bytes())),
            Param::Blob(b) => ToSqlOutput::Borrowed(ValueRef::Blob(b)),
        })
    }
}

impl Param {
    /// The text, if this is text.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Param::Text(s) => Some(s),
            _ => None,
        }
    }

    /// The integer, if this is one.
    #[must_use]
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Param::Int(i) => Some(*i),
            _ => None,
        }
    }

    /// The integer as `u64`, if this is a non-negative integer.
    #[must_use]
    pub fn as_u64(&self) -> Option<u64> {
        self.as_i64().and_then(|i| u64::try_from(i).ok())
    }

    /// True for `NULL`.
    #[must_use]
    pub fn is_null(&self) -> bool {
        matches!(self, Param::Null)
    }
}

/// One result row: the selected columns in order.
#[derive(Debug, Clone, PartialEq)]
pub struct Row(pub Vec<Param>);

impl Row {
    /// Column `i`, or `Null` when the row is shorter.
    #[must_use]
    pub fn get(&self, i: usize) -> &Param {
        self.0.get(i).unwrap_or(&Param::Null)
    }

    /// Column `i` as text.
    #[must_use]
    pub fn text(&self, i: usize) -> Option<&str> {
        self.get(i).as_str()
    }

    /// Column `i` as an integer.
    #[must_use]
    pub fn int(&self, i: usize) -> Option<i64> {
        self.get(i).as_i64()
    }
}

/// One row of 11.17's `head` table: what the index was built from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Head {
    /// `main`, `S<n>`, or `V<n>`.
    pub chain: String,
    /// The last event ID folded.
    pub last_id: String,
    /// Its `seq`.
    pub seq: u64,
    /// Its position's segment (main chain only).
    pub segment: Option<u64>,
    /// Its position's offset (main chain only).
    pub offset: Option<u64>,
    /// Its batch (main chain only).
    pub batch: Option<u64>,
    /// Its epoch (main chain only).
    pub epoch: Option<u64>,
    /// The index schema version the row was written under.
    pub schema_version: u64,
}

/// The index database: one SQLite file per machine and project (3.4).
pub struct Index {
    conn: Connection,
}

impl Index {
    /// Open or create the database at `path`.
    ///
    /// # Errors
    /// Any `rusqlite` error.
    pub fn open(path: &Path) -> Result<Index, Error> {
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        conn.busy_timeout(std::time::Duration::from_secs(10))?;
        Ok(Index { conn })
    }

    /// An in-memory database, for tests and for commands that need no cache.
    ///
    /// # Errors
    /// Any `rusqlite` error.
    pub fn in_memory() -> Result<Index, Error> {
        Ok(Index {
            conn: Connection::open_in_memory()?,
        })
    }

    /// `PRAGMA integrity_check` (3.4): `Ok(true)` when SQLite reports `ok`.
    ///
    /// # Errors
    /// Any `rusqlite` error.
    pub fn integrity_ok(&self) -> Result<bool, Error> {
        let answer: String = self
            .conn
            .query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
        Ok(answer == "ok")
    }

    /// Run statements outside a fold (schema creation, drops at rebuild).
    ///
    /// # Errors
    /// Any `rusqlite` error.
    pub fn execute_batch(&self, sql: &str) -> Result<(), Error> {
        self.conn.execute_batch(sql)
    }

    /// Begin one transaction: the fold contract is one per main-chain batch.
    ///
    /// # Errors
    /// Any `rusqlite` error.
    pub fn begin(&mut self) -> Result<IndexTx<'_>, Error> {
        Ok(IndexTx {
            tx: self.conn.transaction()?,
        })
    }
}

/// One transaction over the index (11.17).
pub struct IndexTx<'a> {
    tx: Transaction<'a>,
}

impl IndexTx<'_> {
    /// Run one statement; returns the number of rows changed.
    ///
    /// # Errors
    /// Any `rusqlite` error.
    pub fn execute(&mut self, sql: &str, params: &[Param]) -> Result<usize, Error> {
        let refs: Vec<&dyn ToSql> = params.iter().map(|p| p as &dyn ToSql).collect();
        self.tx.execute(sql, refs.as_slice())
    }

    /// Run one query and collect every row.
    ///
    /// # Errors
    /// Any `rusqlite` error.
    pub fn query_rows(&mut self, sql: &str, params: &[Param]) -> Result<Vec<Row>, Error> {
        let refs: Vec<&dyn ToSql> = params.iter().map(|p| p as &dyn ToSql).collect();
        let mut stmt = self.tx.prepare(sql)?;
        let width = stmt.column_count();
        let rows = stmt.query_map(refs.as_slice(), |row| {
            let mut cells = Vec::with_capacity(width);
            for i in 0..width {
                cells.push(match row.get_ref(i)? {
                    ValueRef::Null => Param::Null,
                    ValueRef::Integer(v) => Param::Int(v),
                    ValueRef::Real(v) => Param::Text(v.to_string()),
                    ValueRef::Text(t) => Param::Text(String::from_utf8_lossy(t).into_owned()),
                    ValueRef::Blob(b) => Param::Blob(b.to_vec()),
                });
            }
            Ok(Row(cells))
        })?;
        rows.collect()
    }

    /// The `head` row for a chain, if the index has folded it.
    ///
    /// # Errors
    /// Any `rusqlite` error, including an absent `head` table.
    pub fn head(&mut self, chain: &str) -> Result<Option<Head>, Error> {
        let rows = self.query_rows(
            "SELECT chain, last_id, seq, segment, offset, batch, epoch, schema_version \
             FROM head WHERE chain = ?1",
            &[Param::from(chain)],
        )?;
        Ok(rows.first().map(|r| Head {
            chain: r.text(0).unwrap_or_default().to_owned(),
            last_id: r.text(1).unwrap_or_default().to_owned(),
            seq: r.get(2).as_u64().unwrap_or(0),
            segment: r.get(3).as_u64(),
            offset: r.get(4).as_u64(),
            batch: r.get(5).as_u64(),
            epoch: r.get(6).as_u64(),
            schema_version: r.get(7).as_u64().unwrap_or(0),
        }))
    }

    /// Commit.
    ///
    /// # Errors
    /// Any `rusqlite` error.
    pub fn commit(self) -> Result<(), Error> {
        self.tx.commit()
    }

    /// Roll back (also what dropping the transaction does).
    ///
    /// # Errors
    /// Any `rusqlite` error.
    pub fn rollback(self) -> Result<(), Error> {
        self.tx.rollback()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEAD_SQL: &str = "CREATE TABLE head (chain TEXT PRIMARY KEY, last_id TEXT NOT NULL, \
        seq INTEGER NOT NULL, segment INTEGER, offset INTEGER, batch INTEGER, epoch INTEGER, \
        schema_version INTEGER NOT NULL);";

    #[test]
    fn execute_query_and_head() {
        let mut index = Index::in_memory().unwrap();
        index.execute_batch(HEAD_SQL).unwrap();
        assert!(index.integrity_ok().unwrap());
        let mut tx = index.begin().unwrap();
        assert_eq!(tx.head("main").unwrap(), None);
        let n = tx
            .execute(
                "INSERT INTO head VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                &[
                    "main".into(),
                    "sha256:ab".into(),
                    7u64.into(),
                    2u64.into(),
                    4096u64.into(),
                    3u64.into(),
                    1u64.into(),
                    1u64.into(),
                ],
            )
            .unwrap();
        assert_eq!(n, 1);
        let head = tx.head("main").unwrap().unwrap();
        assert_eq!(head.seq, 7);
        assert_eq!(head.segment, Some(2));
        assert_eq!(head.offset, Some(4096));
        let rows = tx
            .query_rows("SELECT chain, NULL, seq FROM head", &[])
            .unwrap();
        assert_eq!(rows[0].text(0), Some("main"));
        assert!(rows[0].get(1).is_null());
        assert_eq!(rows[0].int(2), Some(7));
        tx.commit().unwrap();
        let mut tx = index.begin().unwrap();
        assert!(tx.head("main").unwrap().is_some());
        tx.rollback().unwrap();
    }

    #[test]
    fn params_convert() {
        assert_eq!(Param::from(true), Param::Int(1));
        assert_eq!(Param::from(None::<u64>), Param::Null);
        assert_eq!(Param::from(Some("x")), Param::Text("x".into()));
        assert_eq!(Param::from(u64::MAX), Param::Int(i64::MAX));
    }
}
