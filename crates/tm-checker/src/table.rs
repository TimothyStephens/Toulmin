//! The Checker's working table: typed values in row order with the
//! current column list and key (11.6, 11.18.2), read from the Arrow tables
//! Files hands over (item 47's mapping), and the encoding that carries a
//! table into `DuckDB` for a relational step.
//!
//! Encoding. A `dec` column computed by `derive` holds values of different
//! scales (`10 / 4` is `2.5`, `4 / 4` is `1`, 11.6) and an `int` is of
//! arbitrary size, so neither fits one `DECIMAL(38, s)` column. Every cell
//! therefore reaches `DuckDB` as a value whose order and equality are the
//! 11.18.2 ones: `str` as itself (NFC, compared in binary order, 11.10),
//! `bool` as `BOOLEAN`, and every other type as an order key, a text whose
//! binary order is the values' order and whose equality is value equality
//! (so `1.0` and `1.00`, or `int` `2` and `dec` `2.0`, have one key).
//! `missing`, `nan`, and `inf` are SQL `NULL`, which places them by the
//! `missing` rule of `sort` and `rank` and keeps them out of `join`
//! matches and aggregates (11.18.2 Missing). The typed values themselves
//! stay in Rust: `DuckDB` returns row numbers, ranks, and group results,
//! and the output table is assembled from the input rows.

use std::sync::Arc;

use arrow::array::{ArrayRef, BooleanArray, Float64Array, Int64Array, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use bigdecimal::BigDecimal;
use bigdecimal::num_bigint::Sign;
use bigdecimal::num_traits::Zero;

use tm_formats::Code;
use tm_formats::errors::Error;
use tm_formats::parse_spec::ParseSpec;
use tm_formats::typed_value::{Tag, TypedValue, from_arrow};

/// A table at one step: columns with their types, the key column names,
/// and the rows in the current order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    /// Column names and types, in column order.
    pub columns: Vec<(String, Tag)>,
    /// The key column names, in key order (11.18.2 gives each op's
    /// output key).
    pub key: Vec<String>,
    /// The rows, each one value per column, in the current order.
    pub rows: Vec<Vec<TypedValue>>,
}

impl Table {
    /// Read a parsed table: one value per spec column per row (item 47's
    /// Arrow mapping), the key from the spec, rows in batch order.
    ///
    /// # Errors
    /// `E5001` when the batch lacks a spec column or holds the wrong
    /// Arrow type for one.
    pub fn from_arrow(batch: &RecordBatch, spec: &ParseSpec) -> Result<Table, Error> {
        let mut arrays = Vec::with_capacity(spec.columns.len());
        for c in &spec.columns {
            arrays.push(batch.column_by_name(&c.name).ok_or_else(|| {
                Error::new(Code::E5001, format!("the table has no column {:?}", c.name))
            })?);
        }
        let mut rows = Vec::with_capacity(batch.num_rows());
        for r in 0..batch.num_rows() {
            let mut row = Vec::with_capacity(arrays.len());
            for (a, c) in arrays.iter().zip(&spec.columns) {
                row.push(
                    from_arrow(a.as_ref(), r, c.ty)
                        .map_err(|e| Error::new(Code::E5001, e.message))?,
                );
            }
            rows.push(row);
        }
        Ok(Table {
            columns: spec
                .columns
                .iter()
                .map(|c| (c.name.clone(), c.ty.tag()))
                .collect(),
            key: spec.key.clone(),
            rows,
        })
    }

    /// The index of a column.
    #[must_use]
    pub fn index(&self, name: &str) -> Option<usize> {
        self.columns.iter().position(|(n, _)| n == name)
    }

    /// The type of a column.
    #[must_use]
    pub fn tag(&self, name: &str) -> Option<Tag> {
        self.index(name).map(|i| self.columns[i].1)
    }

    /// The column names.
    #[must_use]
    pub fn names(&self) -> Vec<String> {
        self.columns.iter().map(|(n, _)| n.clone()).collect()
    }

    /// The indexes of the key columns, in key order.
    #[must_use]
    pub fn key_indexes(&self) -> Vec<usize> {
        self.key.iter().filter_map(|k| self.index(k)).collect()
    }

    /// The key tuple of every row, in row order (11.18.2 Key mapping).
    #[must_use]
    pub fn key_tuples(&self) -> Vec<Vec<TypedValue>> {
        let idx = self.key_indexes();
        self.rows
            .iter()
            .map(|r| idx.iter().map(|i| r[*i].clone()).collect())
            .collect()
    }

    /// The table with only the rows at `order`, in that order.
    #[must_use]
    pub fn reorder(&self, order: &[usize]) -> Table {
        Table {
            columns: self.columns.clone(),
            key: self.key.clone(),
            rows: order.iter().map(|i| self.rows[*i].clone()).collect(),
        }
    }
}

/// The order key of a number: `1` for zero; for a value written
/// `0.d1d2...dn * 10^E` with `d1` and `dn` non-zero, `2` then `E` offset
/// into 19 digits then the digits for a positive value, and `0` then the
/// offset negated exponent then the nines' complement of the digits then
/// `~` for a negative one. Binary order of keys is numeric order, and two
/// numbers have one key exactly when they are equal.
#[must_use]
pub fn number_key(d: &BigDecimal) -> String {
    const OFFSET: i128 = 1_000_000_000_000_000_000;
    let (n, scale) = d.as_bigint_and_exponent();
    if n.is_zero() {
        return "1".to_owned();
    }
    let neg = n.sign() == Sign::Minus;
    let all = n.magnitude().to_string();
    let digits = all.trim_end_matches('0');
    let e = i128::try_from(all.len()).unwrap_or(i128::MAX) - i128::from(scale);
    if neg {
        let comp: String = digits
            .bytes()
            .map(|b| char::from(b'9' - (b - b'0')))
            .collect();
        format!("0{:019}{comp}~", OFFSET - e)
    } else {
        format!("2{:019}{digits}", OFFSET + e)
    }
}

/// The `DuckDB` value of a cell: `None` is SQL `NULL`.
#[must_use]
pub fn order_key(v: &TypedValue) -> Option<String> {
    match v {
        TypedValue::Int(n) => Some(number_key(&BigDecimal::new(n.clone(), 0))),
        TypedValue::Dec(d) => Some(number_key(d)),
        TypedValue::Str(s) => Some(s.clone()),
        TypedValue::Date(d) => Some(d.to_string()),
        TypedValue::DateTime(t) => Some(format!(
            "{}T{:02}:{:02}:{:02}.{:09}",
            t.date, t.hour, t.minute, t.second, t.nanos
        )),
        TypedValue::Bool(_)
        | TypedValue::List(_)
        | TypedValue::Missing
        | TypedValue::Nan
        | TypedValue::Inf(_) => None,
    }
}

/// The SQL name of column `j` in an encoded table.
#[must_use]
pub fn col(j: usize) -> String {
    format!("c{j}")
}

/// The SQL name of the double carrying column `j` for `mean` and `median`.
#[must_use]
pub fn dbl(j: usize) -> String {
    format!("d{j}")
}

/// The row-number column of an encoded table.
pub const ROW: &str = "row";
/// The canonical-JSON key-tuple column of an encoded table.
pub const KEY: &str = "key";

/// What to encode beyond the row number and the cells.
#[derive(Debug, Default, Clone)]
pub struct Extras {
    /// Add the key-tuple column `key` (canonical JSON, 11.18.2 `group`).
    pub key_json: bool,
    /// Add a double column `d<j>` for each of these columns.
    pub doubles: Vec<usize>,
}

/// Encode a table for `DuckDB`: `row` (0-based row number, `BIGINT`),
/// then `c<j>` per column (module docs), then the extras.
///
/// # Errors
/// `E5001` when Arrow refuses the batch or a key tuple cannot be written.
pub fn encode(t: &Table, extras: &Extras) -> Result<RecordBatch, Error> {
    let mut fields = vec![Field::new(ROW, DataType::Int64, false)];
    let n = i64::try_from(t.rows.len()).unwrap_or(i64::MAX);
    let mut arrays: Vec<ArrayRef> = vec![Arc::new(Int64Array::from_iter_values(0..n))];
    for (j, (_, tag)) in t.columns.iter().enumerate() {
        if *tag == Tag::Bool {
            fields.push(Field::new(col(j), DataType::Boolean, true));
            arrays.push(Arc::new(BooleanArray::from_iter(t.rows.iter().map(
                |r| match r[j] {
                    TypedValue::Bool(b) => Some(b),
                    _ => None,
                },
            ))));
        } else {
            fields.push(Field::new(col(j), DataType::Utf8, true));
            arrays.push(Arc::new(StringArray::from_iter(
                t.rows.iter().map(|r| order_key(&r[j])),
            )));
        }
    }
    if extras.key_json {
        let idx = t.key_indexes();
        let mut keys = Vec::with_capacity(t.rows.len());
        for r in &t.rows {
            let tuple: Vec<serde_json::Value> = idx.iter().map(|i| r[*i].to_json()).collect();
            keys.push(
                tm_formats::canonical::canonical(&serde_json::Value::Array(tuple))
                    .map_err(|e| Error::new(Code::E5001, e.message))?,
            );
        }
        fields.push(Field::new(KEY, DataType::Utf8, false));
        arrays.push(Arc::new(StringArray::from_iter_values(keys)));
    }
    for j in &extras.doubles {
        fields.push(Field::new(dbl(*j), DataType::Float64, true));
        arrays.push(Arc::new(Float64Array::from_iter(
            t.rows.iter().map(|r| crate::eval::double_or_none(&r[*j])),
        )));
    }
    RecordBatch::try_new(Arc::new(Schema::new(fields)), arrays)
        .map_err(|e| Error::new(Code::E5001, format!("encoding a table for DuckDB: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tm_formats::typed_value::parse_dec_text;

    #[test]
    fn number_keys_order_and_equality() {
        let vals = [
            "-1000", "-12.5", "-12", "-1.25", "-0.003", "0", "0.0001", "0.12", "0.123", "1", "1.5",
            "2", "10", "11", "100.5",
        ];
        let keys: Vec<String> = vals
            .iter()
            .map(|s| number_key(&parse_dec_text(s).unwrap()))
            .collect();
        for w in keys.windows(2) {
            assert!(w[0].as_bytes() < w[1].as_bytes(), "{w:?}");
        }
        assert_eq!(
            number_key(&parse_dec_text("1.500").unwrap()),
            number_key(&parse_dec_text("1.5").unwrap())
        );
        assert_eq!(
            number_key(&parse_dec_text("2").unwrap()),
            number_key(&parse_dec_text("2.00").unwrap())
        );
        assert_eq!(
            number_key(&parse_dec_text("-0.120").unwrap()),
            number_key(&parse_dec_text("-0.12").unwrap())
        );
    }
}
