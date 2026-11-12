//! Toulmin `DuckDB` engine (design 4.1, item 26a, D24): one function that
//! opens an in-memory `DuckDB` with the determinism settings, registers Arrow
//! tables, applies a per-call time limit, and reports the runtime version,
//! loaded extensions, and build flavour. The `duckdb` dependency lives only
//! here; the binding is the only place `unsafe` enters the workspace.

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use arrow::record_batch::RecordBatch;
use duckdb::vtab::arrow::{ArrowBatchRegistration, ArrowVTab};
use duckdb::{Config, Connection};

/// The settings every check runs under (11.19, NFR Determinism).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineSettings {
    /// `checker.memory_limit_mib` (default 2048).
    pub memory_limit_mib: u64,
    /// `checker.temp_directory`: an absolute path (the parse cache directory).
    pub temp_directory: String,
    /// `checker.call_seconds` (default 30, OQ4): each `query` is interrupted
    /// after this long.
    pub call_seconds: u64,
}

impl Default for EngineSettings {
    fn default() -> Self {
        EngineSettings {
            memory_limit_mib: 2048,
            temp_directory: std::env::temp_dir().to_string_lossy().into_owned(),
            call_seconds: 30,
        }
    }
}

/// What the verdict's `versions` block records about the engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineFacts {
    /// `DuckDB`'s library version, for example `v2.0.0`.
    pub version: String,
    /// Loaded extensions, sorted.
    pub extensions: Vec<String>,
    /// `musl` or `glibc`.
    pub build: &'static str,
}

/// An engine error: `DuckDB`'s message, or the call limit.
#[derive(Debug)]
pub enum Error {
    /// `DuckDB` refused.
    Duckdb(duckdb::Error),
    /// The per-call time limit passed and the query was interrupted.
    Timeout(Duration),
    /// The result batches could not be concatenated.
    Arrow(arrow::error::ArrowError),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Duckdb(e) => write!(f, "{e}"),
            Error::Arrow(e) => write!(f, "{e}"),
            Error::Timeout(d) => write!(
                f,
                "query interrupted after {} s (checker.call_seconds)",
                d.as_secs()
            ),
        }
    }
}

impl std::error::Error for Error {}

impl From<duckdb::Error> for Error {
    fn from(e: duckdb::Error) -> Self {
        Error::Duckdb(e)
    }
}

impl From<arrow::error::ArrowError> for Error {
    fn from(e: arrow::error::ArrowError) -> Self {
        Error::Arrow(e)
    }
}

/// One in-memory `DuckDB` with the settings applied.
pub struct Engine {
    conn: Connection,
    settings: EngineSettings,
    registered: Vec<ArrowBatchRegistration>,
}

/// The build flavour of this binary.
#[must_use]
pub const fn build_flavour() -> &'static str {
    if cfg!(target_env = "musl") {
        "musl"
    } else {
        "glibc"
    }
}

/// Open an in-memory engine. Every setting is applied both in the `Config`
/// the connection is created from and by a `SET` afterwards, because the
/// bundled build defaults extension autoinstall and autoload to on (P0).
///
/// # Errors
/// `Error::Duckdb` when `DuckDB` refuses a setting.
pub fn open(settings: EngineSettings) -> Result<Engine, Error> {
    let memory = format!("{}MiB", settings.memory_limit_mib);
    let config = Config::default()
        .threads(1)?
        .max_memory(&memory)?
        .with("temp_directory", &settings.temp_directory)?
        .with("autoinstall_known_extensions", "false")?
        .with("autoload_known_extensions", "false")?
        .enable_autoload_extension(false)?;
    let conn = Connection::open_in_memory_with_flags(config)?;
    conn.execute_batch(&format!(
        "SET threads = 1; SET memory_limit = '{memory}'; SET temp_directory = '{}'; \
         SET autoinstall_known_extensions = false; SET autoload_known_extensions = false;",
        settings.temp_directory.replace('\'', "''")
    ))?;
    conn.register_table_function::<ArrowVTab>("arrow")?;
    Ok(Engine {
        conn,
        settings,
        registered: Vec::new(),
    })
}

impl Engine {
    /// Register an Arrow batch as table `name`. The batch is copied into a
    /// `DuckDB` table of that name, so later queries do not depend on the
    /// Arrow buffers staying alive.
    ///
    /// # Errors
    /// `Error::Duckdb`.
    pub fn register_arrow(&mut self, name: &str, batch: RecordBatch) -> Result<(), Error> {
        let reg = ArrowBatchRegistration::new(batch);
        let sql = format!(
            "CREATE OR REPLACE TABLE {} AS SELECT * FROM arrow(?)",
            quote_ident(name)
        );
        self.conn.execute(&sql, [&reg])?;
        self.registered.push(reg);
        Ok(())
    }

    /// Run one query and collect its result as a single `RecordBatch`
    /// (an empty batch with the result's schema when there are no rows),
    /// interrupted after `call_seconds`.
    ///
    /// # Errors
    /// `Error::Timeout` when the limit passed; `Error::Duckdb` or
    /// `Error::Arrow` otherwise.
    pub fn query(&self, sql: &str) -> Result<RecordBatch, Error> {
        let limit = Duration::from_secs(self.settings.call_seconds.max(1));
        let handle = self.conn.interrupt_handle();
        let fired = Arc::new(AtomicBool::new(false));
        let stop = Arc::new(AtomicBool::new(false));
        let timer = {
            let (fired, stop) = (Arc::clone(&fired), Arc::clone(&stop));
            std::thread::spawn(move || {
                let start = std::time::Instant::now();
                while !stop.load(Ordering::SeqCst) {
                    if start.elapsed() >= limit {
                        fired.store(true, Ordering::SeqCst);
                        handle.interrupt();
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
            })
        };
        let result = self.run(sql);
        stop.store(true, Ordering::SeqCst);
        let _ = timer.join();
        match result {
            Err(Error::Duckdb(_)) if fired.load(Ordering::SeqCst) => Err(Error::Timeout(limit)),
            other => other,
        }
    }

    fn run(&self, sql: &str) -> Result<RecordBatch, Error> {
        let mut stmt = self.conn.prepare(sql)?;
        let batches: Vec<RecordBatch> = stmt.query_arrow([])?.collect();
        let schema = stmt.schema();
        if batches.is_empty() {
            return Ok(RecordBatch::new_empty(schema));
        }
        Ok(arrow::compute::concat_batches(&schema, &batches)?)
    }

    /// Run statements with no result (`CREATE`, `SET`).
    ///
    /// # Errors
    /// `Error::Duckdb`.
    pub fn execute(&self, sql: &str) -> Result<(), Error> {
        self.conn.execute_batch(sql)?;
        Ok(())
    }

    /// Version, loaded extensions, and build flavour.
    ///
    /// # Errors
    /// `Error::Duckdb`.
    pub fn facts(&self) -> Result<EngineFacts, Error> {
        let version: String = self.conn.query_row("SELECT version()", [], |r| r.get(0))?;
        let mut stmt = self.conn.prepare(
            "SELECT extension_name FROM duckdb_extensions() WHERE loaded ORDER BY extension_name",
        )?;
        let extensions: Vec<String> = stmt
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<Result<_, _>>()?;
        Ok(EngineFacts {
            version,
            extensions,
            build: build_flavour(),
        })
    }

    /// The settings as `DuckDB` reports them, for the NFR Budgets snapshot:
    /// `(threads, memory_limit, temp_directory, autoinstall, autoload)`.
    ///
    /// # Errors
    /// `Error::Duckdb`.
    pub fn effective_settings(&self) -> Result<Vec<(String, String)>, Error> {
        let mut stmt = self.conn.prepare(
            "SELECT name, value FROM duckdb_settings() WHERE name IN \
             ('threads', 'memory_limit', 'temp_directory', 'autoinstall_known_extensions', \
              'autoload_known_extensions') ORDER BY name",
        )?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }

    /// The settings this engine was opened with.
    #[must_use]
    pub fn settings(&self) -> &EngineSettings {
        &self.settings
    }
}

/// Double-quote an identifier for SQL.
#[must_use]
pub fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::array::{Int64Array, StringArray};
    use arrow::datatypes::{DataType, Field, Schema};

    fn settings() -> EngineSettings {
        EngineSettings {
            memory_limit_mib: 256,
            temp_directory: std::env::temp_dir().to_string_lossy().into_owned(),
            call_seconds: 1,
        }
    }

    #[test]
    fn settings_are_applied_both_ways() {
        let engine = open(settings()).unwrap();
        let eff = engine.effective_settings().unwrap();
        let get = |k: &str| {
            eff.iter().find(|(n, _)| n == k).map_or_else(
                || panic!("setting {k} not reported; got {eff:?}"),
                |(_, v)| v.clone(),
            )
        };
        assert_eq!(get("threads"), "1");
        assert_eq!(get("autoinstall_known_extensions"), "false");
        assert_eq!(get("autoload_known_extensions"), "false");
        assert!(
            get("memory_limit").contains("256"),
            "{}",
            get("memory_limit")
        );
        assert_eq!(get("temp_directory"), settings().temp_directory);
        let facts = engine.facts().unwrap();
        assert!(facts.version.starts_with('v'), "{}", facts.version);
        assert!(
            facts.extensions.iter().any(|e| e == "parquet"),
            "{:?}",
            facts.extensions
        );
        assert!(["musl", "glibc"].contains(&facts.build));
    }

    #[test]
    fn arrow_round_trip_and_query() {
        let mut engine = open(settings()).unwrap();
        let schema = Arc::new(Schema::new(vec![
            Field::new("gene", DataType::Utf8, false),
            Field::new("count", DataType::Int64, true),
        ]));
        let batch = RecordBatch::try_new(
            schema,
            vec![
                Arc::new(StringArray::from(vec!["b", "a", "c"])),
                Arc::new(Int64Array::from(vec![Some(2), Some(1), None])),
            ],
        )
        .unwrap();
        engine.register_arrow("t\"1", batch).unwrap();
        let out = engine
            .query("SELECT gene, count FROM \"t\"\"1\" ORDER BY gene NULLS LAST")
            .unwrap();
        assert_eq!(out.num_rows(), 3);
        let genes = out
            .column(0)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        assert_eq!(genes.value(0), "a");
        let empty = engine
            .query("SELECT gene FROM \"t\"\"1\" WHERE count > 100")
            .unwrap();
        assert_eq!(empty.num_rows(), 0);
        assert_eq!(empty.num_columns(), 1);
        assert!(matches!(
            engine.query("SELECT * FROM nowhere"),
            Err(Error::Duckdb(_))
        ));
    }

    #[test]
    fn call_limit_interrupts() {
        let engine = open(settings()).unwrap();
        let started = std::time::Instant::now();
        let r = engine.query("SELECT count(*) FROM range(100000000000)");
        assert!(matches!(r, Err(Error::Timeout(_))), "{r:?}");
        assert!(started.elapsed() < Duration::from_secs(20));
    }
}
