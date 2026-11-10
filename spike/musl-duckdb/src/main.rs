//! P0 spike (design item 9): does `duckdb-rs` with the `bundled` feature
//! link statically under musl, with mimalloc as the global allocator and the
//! real work on a spawned thread with an 8 MiB stack, and do the numbers it
//! prints match the glibc builds bit for bit?
//!
//! Throwaway: deleted in the change that lands `docs/decisions/musl-spike.md`.
//!
//! Output format: one `key: value` line per fact, one `row: ...` line per
//! result row, and `time:` lines last. Everything except the `time:` lines
//! must be identical across builds; ci.yml diffs them with the `time:` lines
//! removed.

use std::error::Error;
use std::fmt::Write as _;
use std::time::Instant;

use duckdb::types::Value;
use duckdb::{Config, Connection};

#[cfg(feature = "mimalloc")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

/// Default worker stack (NFR Portability). The first argument overrides it in
/// MiB so step 4's 16 MiB variant needs no rebuild.
const DEFAULT_STACK_MIB: usize = 8;

type SpikeError = Box<dyn Error + Send + Sync>;

const TABLE_SQL: &str = "
CREATE TABLE t (id INTEGER NOT NULL, d DECIMAL(18, 6) NOT NULL, x DOUBLE NOT NULL);
INSERT INTO t VALUES
  (1,     0.000123, 0.1),
  (2,     1.000000, 1.0),
  (3,     2.718282, 2.718281828459045),
  (4,     3.141593, 3.141592653589793),
  (5,    10.500000, 0.00001),
  (6,  1234.567890, 123456.789),
  (7,     0.333333, 0.3333333333333333),
  (8,    99.999999, 7.000000000000001),
  (9,     5.000005, 0.5),
  (10,    2.500000, 2.2250738585072014e-308),
  (11,    7.250000, 1e15),
  (12,    0.500000, 42.42);
";

const ROW_SQL: &str = "
SELECT id,
       d::VARCHAR                          AS d_text,
       ln(d)                               AS ln_d,
       exp(d / 100)                        AS exp_d,
       ln(x)                               AS ln_x,
       exp(x / 1e14)                       AS exp_x,
       ln(d) * exp(x / 1e14) + ln(x)       AS mix
FROM t
ORDER BY id
";

const AGG_SQL: &str = "
SELECT count(*)                            AS n,
       avg(d)::VARCHAR                     AS avg_d_text,
       avg(d)                              AS avg_d,
       avg(x)                              AS avg_x,
       avg(ln(d))                          AS avg_ln_d,
       avg(ln(x))                          AS avg_ln_x,
       avg(exp(d / 100))                   AS avg_exp_d,
       avg(exp(x / 1e14))                  AS avg_exp_x,
       exp(avg(ln(x)))                     AS geomean_x,
       ln(avg(d))                          AS ln_avg_d,
       sum(d)::VARCHAR                     AS sum_d_text,
       stddev_samp(x)                      AS sd_x
FROM t
";

const GROUP_SQL: &str = "
SELECT id % 3                              AS g,
       avg(d)::VARCHAR                     AS avg_d_text,
       avg(d)                              AS avg_d,
       avg(x)                              AS avg_x,
       avg(ln(x))                          AS avg_ln_x,
       exp(avg(ln(d)))                     AS geomean_d
FROM t
GROUP BY g
ORDER BY g
";

fn main() {
    let t0 = Instant::now();
    let stack_mib = std::env::args()
        .nth(1)
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(DEFAULT_STACK_MIB);
    let spawned = std::thread::Builder::new()
        .name("spike-worker".to_owned())
        .stack_size(stack_mib * 1024 * 1024)
        .spawn(move || work(stack_mib));
    let outcome: Result<String, SpikeError> = match spawned {
        Ok(handle) => handle
            .join()
            .unwrap_or_else(|_| Err("worker thread panicked".into())),
        Err(e) => Err(e.into()),
    };
    match outcome {
        Ok(report) => {
            print!("{report}");
            println!("time: total_wall_ms={}", t0.elapsed().as_millis());
        }
        Err(e) => {
            eprintln!("spike failed: {e}");
            std::process::exit(1);
        }
    }
}

fn work(stack_mib: usize) -> Result<String, SpikeError> {
    let mut out = String::new();
    writeln!(out, "build: {}", build_description())?;
    writeln!(out, "worker_stack_mib: {stack_mib}")?;

    let t_open = Instant::now();
    let config = Config::default()
        .threads(1)?
        .enable_autoload_extension(false)?
        .with("autoinstall_known_extensions", "false")?;
    let conn = Connection::open_in_memory_with_flags(config)?;
    conn.execute_batch(
        "SET threads = 1;
         SET autoinstall_known_extensions = false;
         SET autoload_known_extensions = false;",
    )?;
    let open_ms = t_open.elapsed().as_millis();

    let version: String = conn.query_row("SELECT version()", [], |r| r.get(0))?;
    writeln!(out, "duckdb_version: {version}")?;
    for setting in [
        "threads",
        "autoinstall_known_extensions",
        "autoload_known_extensions",
    ] {
        let value: String = conn.query_row(
            &format!("SELECT current_setting('{setting}')::VARCHAR"),
            [],
            |r| r.get(0),
        )?;
        writeln!(out, "setting: {setting}={value}")?;
    }
    let loaded = {
        let mut stmt = conn.prepare(
            "SELECT extension_name FROM duckdb_extensions() WHERE loaded ORDER BY extension_name",
        )?;
        let names = stmt
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        names.join(",")
    };
    writeln!(out, "loaded_extensions: {loaded}")?;

    conn.execute_batch(TABLE_SQL)?;

    let t_query = Instant::now();
    dump(&conn, "rows", ROW_SQL, &mut out)?;
    dump(&conn, "agg", AGG_SQL, &mut out)?;
    dump(&conn, "group", GROUP_SQL, &mut out)?;
    let query_ms = t_query.elapsed().as_millis();

    writeln!(out, "time: open_ms={open_ms}")?;
    writeln!(out, "time: query_ms={query_ms}")?;
    Ok(out)
}

/// Runs `sql` and appends one `row: <label> | name=value ...` line per row.
/// Doubles are printed as the shortest decimal that round-trips plus their
/// raw bits, so two builds agree only when the results are bit-identical.
fn dump(conn: &Connection, label: &str, sql: &str, out: &mut String) -> Result<(), SpikeError> {
    let mut stmt = conn.prepare(sql)?;
    let mut rows = stmt.query([])?;
    let names: Vec<String> = rows
        .as_ref()
        .map(duckdb::Statement::column_names)
        .unwrap_or_default();
    let mut count = 0_usize;
    while let Some(row) = rows.next()? {
        count += 1;
        write!(out, "row: {label} |")?;
        for (i, name) in names.iter().enumerate() {
            let value: Value = row.get(i)?;
            write!(out, " {name}={}", render(&value))?;
        }
        writeln!(out)?;
    }
    writeln!(out, "rows: {label} count={count}")?;
    Ok(())
}

fn render(value: &Value) -> String {
    match value {
        Value::Null => "NULL".to_owned(),
        Value::Double(f) => format!("{f:?}[0x{:016x}]", f.to_bits()),
        Value::Float(f) => format!("{f:?}[0x{:08x}]", f.to_bits()),
        Value::Text(s) => s.clone(),
        Value::Int(v) => v.to_string(),
        Value::BigInt(v) => v.to_string(),
        Value::HugeInt(v) => v.to_string(),
        other => format!("{other:?}"),
    }
}

fn build_description() -> String {
    let allocator = if cfg!(feature = "mimalloc") {
        "mimalloc"
    } else {
        "system"
    };
    let crt = if cfg!(target_feature = "crt-static") {
        "crt-static"
    } else {
        "crt-dynamic"
    };
    format!(
        "{} {} allocator={allocator} {crt} opt={}",
        std::env::consts::ARCH,
        if cfg!(target_env = "musl") {
            "musl"
        } else if cfg!(target_env = "gnu") {
            "gnu"
        } else {
            "other-env"
        },
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
    )
}
