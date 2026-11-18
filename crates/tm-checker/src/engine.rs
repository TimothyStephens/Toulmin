//! The Checker's use of `tm-duckdb` (design item 54, D24): one connection
//! per check, tables registered by name, and the engine's version and
//! extension facts captured for the verdict. Every engine failure is the
//! Checker error `E5001`.

use arrow::array::{Array, Float64Array, Int64Array, ListArray, StringArray};
use arrow::record_batch::RecordBatch;

use tm_duckdb::{Engine, EngineFacts, EngineSettings};
use tm_formats::Code;
use tm_formats::errors::Error;

/// One check's engine.
pub struct CheckEngine {
    engine: Engine,
}

fn e5001(e: impl std::fmt::Display) -> Error {
    Error::new(Code::E5001, format!("checker engine: {e}"))
}

impl CheckEngine {
    /// Open one in-memory connection with the 11.19 settings.
    ///
    /// # Errors
    /// `E5001`.
    pub fn open(settings: &EngineSettings) -> Result<CheckEngine, Error> {
        Ok(CheckEngine {
            engine: tm_duckdb::open(settings.clone()).map_err(e5001)?,
        })
    }

    /// Register `batch` as table `name`.
    ///
    /// # Errors
    /// `E5001`.
    pub fn register(&mut self, name: &str, batch: RecordBatch) -> Result<(), Error> {
        self.engine.register_arrow(name, batch).map_err(e5001)
    }

    /// Run one statement.
    ///
    /// # Errors
    /// `E5001`, the time limit included.
    pub fn query(&self, sql: &str) -> Result<RecordBatch, Error> {
        self.engine.query(sql).map_err(e5001)
    }

    /// Version, extensions, and build flavour.
    ///
    /// # Errors
    /// `E5001`.
    pub fn facts(&self) -> Result<EngineFacts, Error> {
        self.engine.facts().map_err(e5001)
    }
}

/// Column `i` of a result as row numbers (`BIGINT`), `None` for `NULL`.
///
/// # Errors
/// `E5001` when the column is not `BIGINT`.
pub fn rows(batch: &RecordBatch, i: usize) -> Result<Vec<Option<usize>>, Error> {
    let a = batch
        .column(i)
        .as_any()
        .downcast_ref::<Int64Array>()
        .ok_or_else(|| e5001(format!("result column {i} is not BIGINT")))?;
    Ok((0..a.len())
        .map(|r| {
            if a.is_null(r) {
                None
            } else {
                usize::try_from(a.value(r)).ok()
            }
        })
        .collect())
}

/// Column `i` of a result as text.
///
/// # Errors
/// `E5001` when the column is not text.
pub fn texts(batch: &RecordBatch, i: usize) -> Result<Vec<Option<String>>, Error> {
    let a = batch
        .column(i)
        .as_any()
        .downcast_ref::<StringArray>()
        .ok_or_else(|| e5001(format!("result column {i} is not text")))?;
    Ok((0..a.len())
        .map(|r| (!a.is_null(r)).then(|| a.value(r).to_owned()))
        .collect())
}

/// Column `i` of a result as doubles.
///
/// # Errors
/// `E5001` when the column is not `DOUBLE`.
pub fn doubles(batch: &RecordBatch, i: usize) -> Result<Vec<Option<f64>>, Error> {
    let a = batch
        .column(i)
        .as_any()
        .downcast_ref::<Float64Array>()
        .ok_or_else(|| e5001(format!("result column {i} is not DOUBLE")))?;
    Ok((0..a.len())
        .map(|r| (!a.is_null(r)).then(|| a.value(r)))
        .collect())
}

/// Column `i` of a result as lists of row numbers.
///
/// # Errors
/// `E5001` when the column is not a list of `BIGINT`.
pub fn row_lists(batch: &RecordBatch, i: usize) -> Result<Vec<Vec<usize>>, Error> {
    let a = batch
        .column(i)
        .as_any()
        .downcast_ref::<ListArray>()
        .ok_or_else(|| e5001(format!("result column {i} is not a list")))?;
    let mut out = Vec::with_capacity(a.len());
    for r in 0..a.len() {
        let v = a.value(r);
        let ints = v
            .as_any()
            .downcast_ref::<Int64Array>()
            .ok_or_else(|| e5001(format!("result column {i} is not a list of BIGINT")))?;
        out.push(
            (0..ints.len())
                .filter_map(|k| usize::try_from(ints.value(k)).ok())
                .collect(),
        );
    }
    Ok(out)
}
