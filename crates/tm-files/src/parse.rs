//! Table parsing (item 47; 11.6; 11.7): CSV and TSV through `DuckDB`'s
//! `read_csv` with every guess off and every column read as text, Parquet
//! through the `parquet` Arrow reader; each cell then becomes a typed value
//! under the spec and the columns become Arrow arrays of item 47's types.
//! `E3101` for a duplicate key, `E3102` for a cell that does not read as
//! its column's type, `E3103` for a spec column the file lacks.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::Path;
use std::sync::Arc;

use arrow::array::{Array, ArrayRef, AsArray, RecordBatch};
use arrow::datatypes::{self as dt, DataType, Field, Schema, TimeUnit};
use bigdecimal::BigDecimal;
use bigdecimal::num_bigint::BigInt;
use serde_json::Value;
use tm_duckdb::EngineSettings;
use tm_formats::parse_spec::{Column, Format, ParseSpec};
use tm_formats::typed_value::{
    Date, DateTime, SpecType, TypedValue, arrow_type, dec_from_f64_shortest, dec_text, from_arrow,
    parse_dec_text, to_arrow,
};

use crate::{Code, Error, Result, read_file};

/// A parsed table with the spec it was parsed under.
#[derive(Debug, Clone)]
pub struct Table {
    /// Columns in spec order, item 47's Arrow types.
    pub batch: RecordBatch,
    /// The spec, scales included.
    pub spec: ParseSpec,
}

/// One cell before typing.
#[derive(Debug, Clone, PartialEq)]
enum Cell {
    /// Text from a CSV or TSV field, or a Parquet string column.
    Text(String),
    /// A typed value from a typed Parquet column.
    Value(TypedValue),
}

fn e3102(row: usize, column: &str, why: impl std::fmt::Display) -> Error {
    Error::code(Code::E3102, format!("row {row}, column {column:?}: {why}"))
}

// ------------------------------------------------------------ CSV and TSV

/// A SQL string literal in `DuckDB`'s escape form.
fn sql_str(s: &str) -> String {
    let mut out = String::from("E'");
    for c in s.chars() {
        match c {
            '\'' => out.push_str("\\'"),
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c => out.push(c),
        }
    }
    out.push('\'');
    out
}

/// Split one header line into RFC 4180 fields.
fn split_fields(line: &str, delim: char) -> Vec<String> {
    let mut fields = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    cur.push('"');
                    chars.next();
                } else {
                    quoted = false;
                }
            } else {
                cur.push(c);
            }
        } else if c == '"' {
            quoted = true;
        } else if c == delim {
            fields.push(std::mem::take(&mut cur));
        } else {
            cur.push(c);
        }
    }
    fields.push(cur);
    fields
}

/// The file's header names (the first line split on the delimiter), or the
/// spec's column names in order when the file has no header.
fn header_names(path: &Path, spec: &ParseSpec) -> Result<Vec<String>> {
    if spec.header != Some(true) {
        return Ok(spec.columns.iter().map(|c| c.name.clone()).collect());
    }
    let bytes = read_file(path)?;
    let end = bytes
        .iter()
        .position(|b| *b == b'\n')
        .unwrap_or(bytes.len());
    let line = std::str::from_utf8(&bytes[..end])
        .map_err(|_| e3102(0, "header", "the header is not UTF-8"))?
        .trim_end_matches('\r');
    let line = line.strip_prefix('\u{feff}').unwrap_or(line);
    Ok(split_fields(line, spec.delimiter.unwrap_or(','))
        .into_iter()
        .map(|f| tm_formats::sort_order::nfc(&f))
        .collect())
}

fn read_csv_source(path: &Path, spec: &ParseSpec, names: &[String]) -> String {
    let columns: Vec<String> = names
        .iter()
        .map(|n| format!("{}: 'VARCHAR'", sql_str(n)))
        .collect();
    format!(
        "read_csv({}, header={}, delim={}, quote='\"', escape='\"', all_varchar=true, auto_detect=false, \
         ignore_errors=false, null_padding=false, columns={{{}}})",
        sql_str(&path.to_string_lossy()),
        spec.header.unwrap_or(false),
        sql_str(&spec.delimiter.unwrap_or(',').to_string()),
        columns.join(", ")
    )
}

/// Every spec column of a text table as rows of cells.
fn text_cells(path: &Path, spec: &ParseSpec, settings: &EngineSettings) -> Result<Vec<Vec<Cell>>> {
    let names = header_names(path, spec)?;
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    spec.check_header(&refs)?;
    let engine = tm_duckdb::open(settings.clone())?;
    let selected: Vec<String> = spec
        .columns
        .iter()
        .map(|c| tm_duckdb::quote_ident(&c.name))
        .collect();
    let sql = format!(
        "SELECT {} FROM {}",
        selected.join(", "),
        read_csv_source(path, spec, &names)
    );
    let batch = engine.query(&sql).map_err(|e| match e {
        tm_duckdb::Error::Duckdb(d) => {
            e3102(0, "table", format!("cannot read the text table: {d}"))
        }
        other => Error::Engine(other),
    })?;
    let mut columns: Vec<Vec<Option<String>>> = Vec::with_capacity(batch.num_columns());
    for col in batch.columns() {
        let col = arrow::compute::cast(col, &DataType::Utf8)?;
        let strings = col.as_string::<i32>();
        columns.push(
            (0..strings.len())
                .map(|i| strings.is_valid(i).then(|| strings.value(i).to_owned()))
                .collect(),
        );
    }
    let rows = batch.num_rows();
    Ok((0..rows)
        .map(|r| {
            columns
                .iter()
                .map(|c| Cell::Text(c[r].clone().unwrap_or_default()))
                .collect()
        })
        .collect())
}

// ------------------------------------------------------------ Parquet

fn parquet_cell(array: &ArrayRef, row: usize, column: &str, row_no: usize) -> Result<Cell> {
    if array.is_null(row) {
        return Ok(Cell::Value(TypedValue::Missing));
    }
    let float = |x: f64| -> Cell {
        if x.is_nan() {
            Cell::Value(TypedValue::Nan)
        } else if x.is_infinite() {
            Cell::Value(TypedValue::Inf(x > 0.0))
        } else {
            dec_from_f64_shortest(x).map_or(Cell::Value(TypedValue::Missing), |d| {
                Cell::Value(TypedValue::Dec(d))
            })
        }
    };
    let int = |n: i128| Cell::Value(TypedValue::Int(BigInt::from(n)));
    Ok(match array.data_type() {
        DataType::Utf8 => Cell::Text(array.as_string::<i32>().value(row).to_owned()),
        DataType::LargeUtf8 => Cell::Text(array.as_string::<i64>().value(row).to_owned()),
        DataType::Utf8View => Cell::Text(array.as_string_view().value(row).to_owned()),
        DataType::Boolean => Cell::Value(TypedValue::Bool(array.as_boolean().value(row))),
        DataType::Int8 => int(i128::from(array.as_primitive::<dt::Int8Type>().value(row))),
        DataType::Int16 => int(i128::from(array.as_primitive::<dt::Int16Type>().value(row))),
        DataType::Int32 => int(i128::from(array.as_primitive::<dt::Int32Type>().value(row))),
        DataType::Int64 => int(i128::from(array.as_primitive::<dt::Int64Type>().value(row))),
        DataType::UInt8 => int(i128::from(array.as_primitive::<dt::UInt8Type>().value(row))),
        DataType::UInt16 => int(i128::from(
            array.as_primitive::<dt::UInt16Type>().value(row),
        )),
        DataType::UInt32 => int(i128::from(
            array.as_primitive::<dt::UInt32Type>().value(row),
        )),
        DataType::UInt64 => int(i128::from(
            array.as_primitive::<dt::UInt64Type>().value(row),
        )),
        DataType::Float32 => float(f64::from(
            array.as_primitive::<dt::Float32Type>().value(row),
        )),
        DataType::Float64 => float(array.as_primitive::<dt::Float64Type>().value(row)),
        DataType::Decimal128(_, s) => Cell::Value(TypedValue::Dec(BigDecimal::new(
            BigInt::from(array.as_primitive::<dt::Decimal128Type>().value(row)),
            i64::from(*s),
        ))),
        DataType::Date32 => {
            let days = i64::from(array.as_primitive::<dt::Date32Type>().value(row));
            Cell::Value(TypedValue::Date(
                Date::from_days(days).ok_or_else(|| e3102(row_no, column, "date out of range"))?,
            ))
        }
        DataType::Date64 => {
            let ms = array.as_primitive::<dt::Date64Type>().value(row);
            Cell::Value(TypedValue::Date(
                Date::from_days(ms.div_euclid(86_400_000))
                    .ok_or_else(|| e3102(row_no, column, "date out of range"))?,
            ))
        }
        DataType::Timestamp(unit, _) => {
            let raw = match unit {
                TimeUnit::Second => array
                    .as_primitive::<dt::TimestampSecondType>()
                    .value(row)
                    .checked_mul(1_000_000_000),
                TimeUnit::Millisecond => array
                    .as_primitive::<dt::TimestampMillisecondType>()
                    .value(row)
                    .checked_mul(1_000_000),
                TimeUnit::Microsecond => array
                    .as_primitive::<dt::TimestampMicrosecondType>()
                    .value(row)
                    .checked_mul(1_000),
                TimeUnit::Nanosecond => Some(
                    array
                        .as_primitive::<dt::TimestampNanosecondType>()
                        .value(row),
                ),
            };
            let nanos = raw.ok_or_else(|| e3102(row_no, column, "timestamp out of range"))?;
            Cell::Value(TypedValue::DateTime(
                DateTime::from_epoch_nanos(nanos)
                    .ok_or_else(|| e3102(row_no, column, "timestamp out of range"))?,
            ))
        }
        other => {
            return Err(e3102(
                row_no,
                column,
                format!("Parquet column type {other} is not read (item 47)"),
            ));
        }
    })
}

/// Every spec column of a Parquet file as rows of cells.
fn parquet_cells(path: &Path, spec: &ParseSpec) -> Result<Vec<Vec<Cell>>> {
    let file = std::fs::File::open(path).map_err(|e| Error::io(path, e))?;
    let batch = crate::parse_cache::read_parquet(file)?;
    let schema = batch.schema();
    let owned: Vec<String> = schema.fields().iter().map(|f| f.name().clone()).collect();
    let refs: Vec<&str> = owned.iter().map(String::as_str).collect();
    spec.check_header(&refs)?;
    let arrays: Vec<&ArrayRef> = spec
        .columns
        .iter()
        .map(|c| {
            batch.column_by_name(&c.name).ok_or_else(|| {
                Error::at(
                    Code::E3103,
                    "/columns",
                    format!("column {:?} is not in the file", c.name),
                )
            })
        })
        .collect::<Result<_>>()?;
    let mut rows = Vec::with_capacity(batch.num_rows());
    for r in 0..batch.num_rows() {
        let mut row = Vec::with_capacity(arrays.len());
        for (a, c) in arrays.iter().zip(&spec.columns) {
            row.push(parquet_cell(a, r, &c.name, r + 1)?);
        }
        rows.push(row);
    }
    Ok(rows)
}

fn cells(path: &Path, spec: &ParseSpec, settings: &EngineSettings) -> Result<Vec<Vec<Cell>>> {
    match spec.format {
        Format::Csv | Format::Tsv => text_cells(path, spec, settings),
        Format::Parquet => parquet_cells(path, spec),
    }
}

// ------------------------------------------------------------ typing

fn special_token(token: &str) -> Option<TypedValue> {
    match token.to_ascii_lowercase().as_str() {
        "nan" => Some(TypedValue::Nan),
        "inf" | "+inf" | "infinity" | "+infinity" => Some(TypedValue::Inf(true)),
        "-inf" | "-infinity" => Some(TypedValue::Inf(false)),
        _ => None,
    }
}

/// Fraction digits of a decimal text (`0` without a point).
fn fraction_len(text: &str) -> usize {
    text.split_once('.').map_or(0, |(_, f)| f.len())
}

fn type_cell(cell: &Cell, column: &Column, spec: &ParseSpec, row: usize) -> Result<TypedValue> {
    let t = column.ty;
    let text = match cell {
        Cell::Value(TypedValue::Missing) => return Ok(TypedValue::Missing),
        Cell::Value(v) => {
            // A typed Parquet cell: accept it when it fits the column's type.
            let ok = matches!(
                (t, v),
                (SpecType::Int, TypedValue::Int(_))
                    | (SpecType::Str, TypedValue::Str(_))
                    | (SpecType::Bool, TypedValue::Bool(_))
                    | (SpecType::Date, TypedValue::Date(_))
                    | (SpecType::DateTime, TypedValue::DateTime(_))
                    | (
                        SpecType::Dec | SpecType::Float64,
                        TypedValue::Int(_) | TypedValue::Dec(_)
                    )
                    | (SpecType::Float64, TypedValue::Nan | TypedValue::Inf(_))
            );
            if !ok {
                return Err(e3102(
                    row,
                    &column.name,
                    format!("a {:?} value in a {} column", v.tag(), t.as_str()),
                ));
            }
            return Ok(match (t, v) {
                (SpecType::Dec | SpecType::Float64, TypedValue::Int(n)) => {
                    TypedValue::Dec(BigDecimal::from(n.clone()))
                }
                _ => v.clone(),
            });
        }
        Cell::Text(s) => s.as_str(),
    };
    if column.missing.iter().any(|m| m == text) {
        return Ok(TypedValue::Missing);
    }
    if let Some(sp) = special_token(text) {
        if t != SpecType::Float64 {
            return Err(e3102(
                row,
                &column.name,
                format!(
                    "{text:?} in a {} column; declare float64 or list it under missing",
                    t.as_str()
                ),
            ));
        }
        let policy = if matches!(sp, TypedValue::Nan) {
            spec.nan
        } else {
            spec.inf
        };
        return Ok(match policy {
            tm_formats::parse_spec::SpecialAs::Missing => TypedValue::Missing,
            tm_formats::parse_spec::SpecialAs::Value => sp,
        });
    }
    if t == SpecType::Float64 {
        let x: f64 = text
            .trim()
            .parse()
            .map_err(|_| e3102(row, &column.name, format!("{text:?} is not a float64")))?;
        if !x.is_finite() {
            return Err(e3102(row, &column.name, format!("{text:?} is not finite")));
        }
        return dec_from_f64_shortest(x)
            .map(TypedValue::Dec)
            .ok_or_else(|| e3102(row, &column.name, format!("{text:?} is not a float64")));
    }
    TypedValue::from_text(text, t)
        .ok_or_else(|| e3102(row, &column.name, format!("{text:?} is not {}", t.as_str())))
}

fn typed_table(rows: &[Vec<Cell>], spec: &ParseSpec) -> Result<Vec<Vec<TypedValue>>> {
    let mut out = Vec::with_capacity(rows.len());
    for (i, row) in rows.iter().enumerate() {
        let mut typed = Vec::with_capacity(row.len());
        for (cell, column) in row.iter().zip(&spec.columns) {
            let v = type_cell(cell, column, spec, i + 1)?;
            if let (SpecType::Dec, TypedValue::Dec(d), Some(scale)) = (column.ty, &v, column.scale)
                && fraction_len(&dec_text(d)) > usize::from(scale)
            {
                return Err(e3102(
                    i + 1,
                    &column.name,
                    format!(
                        "{} has more fraction digits than the spec's scale {scale}",
                        dec_text(d)
                    ),
                ));
            }
            typed.push(v);
        }
        out.push(typed);
    }
    Ok(out)
}

/// The positions of the key columns in spec order.
#[must_use]
pub fn key_indexes(spec: &ParseSpec) -> Vec<usize> {
    spec.key
        .iter()
        .filter_map(|k| spec.columns.iter().position(|c| &c.name == k))
        .collect()
}

/// A row's key tuple as canonical JSON (the `claim_keys.key_tuple` form).
#[must_use]
pub fn key_tuple_json(row: &[TypedValue], key_indexes: &[usize]) -> String {
    let list: Vec<Value> = key_indexes.iter().map(|i| row[*i].to_json()).collect();
    tm_formats::canonical::canonical(&Value::Array(list)).unwrap_or_default()
}

fn check_keys(rows: &[Vec<TypedValue>], spec: &ParseSpec) -> Result<Vec<Vec<TypedValue>>> {
    let idx = key_indexes(spec);
    let mut seen: HashMap<String, usize> = HashMap::with_capacity(rows.len());
    let mut keys = Vec::with_capacity(rows.len());
    for (i, row) in rows.iter().enumerate() {
        let key: Vec<TypedValue> = idx.iter().map(|k| row[*k].clone()).collect();
        if key.iter().any(|v| {
            matches!(
                v,
                TypedValue::Missing | TypedValue::Nan | TypedValue::Inf(_)
            )
        }) {
            return Err(e3102(i + 1, &spec.key.join(","), "a key value is missing"));
        }
        let json = key_tuple_json(row, &idx);
        if let Some(first) = seen.insert(json.clone(), i + 1) {
            return Err(Error::code(
                Code::E3101,
                format!("duplicate key {json} at rows {first} and {}", i + 1),
            ));
        }
        keys.push(key);
    }
    Ok(keys)
}

fn batch_of(rows: &[Vec<TypedValue>], spec: &ParseSpec) -> Result<RecordBatch> {
    let mut fields = Vec::with_capacity(spec.columns.len());
    let mut arrays: Vec<ArrayRef> = Vec::with_capacity(spec.columns.len());
    for (ci, c) in spec.columns.iter().enumerate() {
        let scale = c.scale.unwrap_or(0);
        let values: Vec<TypedValue> = rows.iter().map(|r| r[ci].clone()).collect();
        arrays.push(to_arrow(&values, c.ty, scale)?);
        fields.push(Field::new(c.name.clone(), arrow_type(c.ty, scale), true));
    }
    Ok(RecordBatch::try_new(Arc::new(Schema::new(fields)), arrays)?)
}

/// Parse `path` under `spec`: the table as item 47's Arrow types, columns
/// in spec order, after the key uniqueness check.
///
/// # Errors
/// `E3101` for a duplicate key (naming the tuple and both rows); `E3102`
/// for a cell that does not read as its column's type (naming row and
/// column); `E3103` for a spec column the file lacks; `Error::Engine`,
/// `Error::Parquet`, `Error::Io`.
pub fn parse(path: &Path, spec: &ParseSpec, settings: &EngineSettings) -> Result<RecordBatch> {
    let raw = cells(path, spec, settings)?;
    let rows = typed_table(&raw, spec)?;
    check_keys(&rows, spec)?;
    batch_of(&rows, spec)
}

/// Complete an owner's draft spec for `path` (`tm table spec`, item 47):
/// every `dec` column gets `scale`, the larger of the draft's (when it
/// gives one) and the largest fraction length seen over the column; the
/// result is validated and the file is parsed once to check the header,
/// the types, and the key.
///
/// # Errors
/// `E3105` and the other 11.7 codes for the draft; [`parse`]'s codes for
/// the file.
pub fn complete_spec(path: &Path, draft: &Value, settings: &EngineSettings) -> Result<Table> {
    // A provisional spec with scale 0 wherever a dec column lacks one, so
    // the draft validates and the cells can be read.
    let mut provisional = draft.clone();
    let dec_columns: Vec<usize> = provisional
        .get("columns")
        .and_then(Value::as_array)
        .map(|cols| {
            cols.iter()
                .enumerate()
                .filter(|(_, c)| c.get("type").and_then(Value::as_str) == Some("dec"))
                .map(|(i, _)| i)
                .collect()
        })
        .unwrap_or_default();
    for &i in &dec_columns {
        if let Some(c) = provisional["columns"][i].as_object_mut() {
            c.insert("scale".into(), Value::from(0));
        }
    }
    let spec0 = ParseSpec::from_json(&provisional)?;
    let raw = cells(path, &spec0, settings)?;
    // The largest fraction length per dec column.
    let mut completed = draft.clone();
    for &i in &dec_columns {
        let column = &spec0.columns[i];
        let reserved = draft["columns"][i]["scale"]
            .as_u64()
            .and_then(|s| usize::try_from(s).ok())
            .unwrap_or(0);
        let mut scale = reserved;
        for (r, row) in raw.iter().enumerate() {
            let text = match &row[i] {
                Cell::Text(s) => {
                    if column.missing.iter().any(|m| m == s) {
                        continue;
                    }
                    s.clone()
                }
                Cell::Value(TypedValue::Dec(d)) => dec_text(d),
                Cell::Value(TypedValue::Int(n)) => n.to_string(),
                Cell::Value(TypedValue::Missing) => continue,
                Cell::Value(v) => {
                    return Err(e3102(
                        r + 1,
                        &column.name,
                        format!("a {:?} value in a dec column", v.tag()),
                    ));
                }
            };
            if parse_dec_text(text.trim()).is_none() {
                return Err(e3102(r + 1, &column.name, format!("{text:?} is not dec")));
            }
            scale = scale.max(fraction_len(text.trim()));
        }
        if scale > 38 {
            return Err(e3102(0, &column.name, "a scale above 38"));
        }
        if let Some(c) = completed["columns"][i].as_object_mut() {
            c.insert("scale".into(), Value::from(scale));
        }
    }
    let spec = ParseSpec::from_json(&completed)?;
    let rows = typed_table(&raw, &spec)?;
    check_keys(&rows, &spec)?;
    let batch = batch_of(&rows, &spec)?;
    Ok(Table { batch, spec })
}

/// The rows of a parsed batch as typed values, columns in spec order.
///
/// # Errors
/// `E3102` if the batch is not of the spec's item 47 types.
pub fn typed_rows(batch: &RecordBatch, spec: &ParseSpec) -> Result<Vec<Vec<TypedValue>>> {
    let mut rows = Vec::with_capacity(batch.num_rows());
    for r in 0..batch.num_rows() {
        let mut row = Vec::with_capacity(spec.columns.len());
        for c in &spec.columns {
            let array = batch.column_by_name(&c.name).ok_or_else(|| {
                Error::code(
                    Code::E3102,
                    format!("column {:?} is not in the batch", c.name),
                )
            })?;
            row.push(from_arrow(array.as_ref(), r, c.ty)?);
        }
        rows.push(row);
    }
    Ok(rows)
}

/// The key tuples of a parsed batch, one per row in row order.
///
/// # Errors
/// As [`typed_rows`].
pub fn key_tuples(batch: &RecordBatch, spec: &ParseSpec) -> Result<Vec<Vec<TypedValue>>> {
    let idx = key_indexes(spec);
    Ok(typed_rows(batch, spec)?
        .into_iter()
        .map(|row| idx.iter().map(|i| row[*i].clone()).collect())
        .collect())
}

/// A one-line summary of a batch for messages and tests: `rows x columns`.
#[must_use]
pub fn shape(batch: &RecordBatch) -> String {
    let mut s = String::new();
    let _ = write!(s, "{} x {}", batch.num_rows(), batch.num_columns());
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_fields_and_sql_literals() {
        assert_eq!(split_fields("a,b,c", ','), ["a", "b", "c"]);
        assert_eq!(
            split_fields("\"x,y\",\"q\"\"r\",", ','),
            ["x,y", "q\"r", ""]
        );
        assert_eq!(split_fields("a\tb", '\t'), ["a", "b"]);
        assert_eq!(sql_str("it's\t"), "E'it\\'s\\t'");
        assert_eq!(fraction_len("1.250"), 3);
        assert_eq!(fraction_len("12"), 0);
        assert_eq!(special_token("NaN"), Some(TypedValue::Nan));
        assert_eq!(special_token("-Inf"), Some(TypedValue::Inf(false)));
        assert_eq!(special_token("info"), None);
    }
}
