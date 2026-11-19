//! Item 89: the differential suite's case generator. Writes N case
//! directories in item 92's layout (`tables/`, `specs/`, `steps.json`,
//! `assertion.json`; never `expected.json`) plus a `gen.json` naming the
//! seed, index, and bucket that reproduce each one.
//!
//! `cargo run -p tm-checker --example gencases -- --out DIR --n N [--seed SEED] [--start I]`
//!
//! Case `i` falls in bucket `i mod 16` (the list in `BUCKETS`, item 89's
//! order) and its random stream is `SplitMix64` seeded from `(SEED, i)`, so
//! a case depends on the seed and its index alone. A bucket fixes the shape
//! of its steps; everything else is drawn from the bucket's lists. Every
//! bucket but `exponent`, `float64`, and `overflow` checks its steps and
//! assertion with `tm-formats`' validation and redraws (next attempt, same
//! seed) up to twenty times before accepting an error case.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    reason = "random draws are reduced modulo small bounds; the casts are the reduction"
)]

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde_json::{Value, json};

use tm_formats::parse_spec::ParseSpec;
use tm_formats::steps::TableSchema;
use tm_formats::typed_value::{SpecType, Tag, dec_from_f64_shortest, dec_text};

/// Item 89's coverage buckets, in order; case `i` is in bucket `i mod 16`.
const BUCKETS: [&str; 16] = [
    "filter",
    "derive",
    "logexp",
    "sort-take",
    "rank-all",
    "rank-group",
    "group",
    "join",
    "select",
    "exponent",
    "float64",
    "digit34",
    "missing-group",
    "missing-key-tie",
    "overflow",
    "pipeline",
];

/// Buckets that produce an error verdict on purpose: no redraw.
const ERROR_BUCKETS: [&str; 3] = ["exponent", "float64", "overflow"];

const REDRAWS: u64 = 20;

// ------------------------------------------------------------------ RNG

/// `SplitMix64`.
struct Rng(u64);

fn mix64(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

impl Rng {
    fn for_case(seed: u64, index: u64, attempt: u64) -> Rng {
        let a = mix64(seed.wrapping_add(0x9E37_79B9_7F4A_7C15));
        let b = mix64(
            index
                .wrapping_mul(0xD1B5_4A32_D192_ED03)
                .wrapping_add(0x1234_5678),
        );
        let c = mix64(attempt.wrapping_mul(0x8CB9_2BA7_2F3D_8DD7).wrapping_add(7));
        Rng(a ^ b.rotate_left(17) ^ c.rotate_left(41))
    }

    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        mix64(self.0)
    }

    /// Uniform in `0..n`; `n` must be positive.
    fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            return 0;
        }
        (self.next() % n as u64) as usize
    }

    /// Uniform in `lo..=hi`.
    fn range(&mut self, lo: i64, hi: i64) -> i64 {
        if hi <= lo {
            return lo;
        }
        lo + (self.next() % ((hi - lo + 1) as u64)) as i64
    }

    /// True with probability `percent` in 100.
    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }

    fn pick<'a, T>(&mut self, xs: &'a [T]) -> &'a T {
        &xs[self.below(xs.len())]
    }

    fn pick_owned<T: Clone>(&mut self, xs: &[T]) -> T {
        xs[self.below(xs.len())].clone()
    }

    fn shuffle<T>(&mut self, xs: &mut [T]) {
        for i in (1..xs.len()).rev() {
            let j = self.below(i + 1);
            xs.swap(i, j);
        }
    }

    /// `k` distinct draws from `xs` (fewer when `xs` is shorter).
    fn sample<T: Clone>(&mut self, xs: &[T], k: usize) -> Vec<T> {
        let mut v = xs.to_vec();
        self.shuffle(&mut v);
        v.truncate(k);
        v
    }
}

// ------------------------------------------------------------- tables

/// One column of a generated table: its spec entry and its cells.
#[derive(Clone)]
struct Column {
    name: String,
    ty: SpecType,
    /// The `dec` scale; 0 for every other type.
    scale: u8,
    /// The text written into the file, one per row.
    tokens: Vec<String>,
    /// The typed value of each cell; `None` for `missing`, `nan`, and `inf`.
    values: Vec<Option<Value>>,
}

impl Column {
    fn has_specials(&self) -> bool {
        self.values.iter().any(Option::is_none)
    }
    fn present(&self) -> Vec<Value> {
        self.values.iter().flatten().cloned().collect()
    }
}

#[derive(Clone)]
struct Table {
    name: String,
    columns: Vec<Column>,
    /// Indexes into `columns`.
    key: Vec<usize>,
    tsv: bool,
    nan_value: bool,
    inf_value: bool,
    missing_tokens: Vec<String>,
}

impl Table {
    fn rows(&self) -> usize {
        self.columns.first().map_or(0, |c| c.tokens.len())
    }

    fn spec_json(&self) -> Value {
        let columns: Vec<Value> = self
            .columns
            .iter()
            .map(|c| {
                let mut o =
                    json!({"name": c.name, "type": c.ty.as_str(), "missing": self.missing_tokens});
                if c.ty == SpecType::Dec
                    && let Some(m) = o.as_object_mut()
                {
                    m.insert("scale".into(), json!(c.scale));
                }
                o
            })
            .collect();
        let key: Vec<&str> = self
            .key
            .iter()
            .map(|&i| self.columns[i].name.as_str())
            .collect();
        json!({
            "spec": 1,
            "format": if self.tsv { "tsv" } else { "csv" },
            "delimiter": if self.tsv { "\t" } else { "," },
            "header": true,
            "columns": columns,
            "key": key,
            "nan": if self.nan_value { "value" } else { "missing" },
            "inf": if self.inf_value { "value" } else { "missing" },
            "encoding": "utf-8",
        })
    }

    fn file_text(&self) -> String {
        let sep = if self.tsv { "\t" } else { "," };
        let mut out = String::new();
        let header: Vec<&str> = self.columns.iter().map(|c| c.name.as_str()).collect();
        out.push_str(&header.join(sep));
        out.push('\n');
        for r in 0..self.rows() {
            let row: Vec<&str> = self.columns.iter().map(|c| c.tokens[r].as_str()).collect();
            out.push_str(&row.join(sep));
            out.push('\n');
        }
        out
    }

    fn file_name(&self) -> String {
        format!("{}.{}", self.name, if self.tsv { "tsv" } else { "csv" })
    }

    fn schema(&self) -> Schema {
        let mut pools = BTreeMap::new();
        let mut specials = BTreeSet::new();
        for c in &self.columns {
            pools.insert(c.name.clone(), c.present());
            if c.has_specials() {
                specials.insert(c.name.clone());
            }
        }
        Schema {
            columns: self
                .columns
                .iter()
                .map(|c| (c.name.clone(), c.ty.tag()))
                .collect(),
            key: self
                .key
                .iter()
                .map(|&i| self.columns[i].name.clone())
                .collect(),
            pools,
            specials,
            ordered: false,
        }
    }

    /// The first non-key column of a type, else the first non-key column,
    /// else the first column.
    fn first_of(&self, ty: SpecType) -> String {
        let nonkey = |(i, _): &(usize, &Column)| !self.key.contains(i);
        self.columns
            .iter()
            .enumerate()
            .filter(nonkey)
            .find(|(_, c)| c.ty == ty)
            .or_else(|| self.columns.iter().enumerate().find(nonkey))
            .map_or_else(|| self.columns[0].name.clone(), |(_, c)| c.name.clone())
    }
}

/// A key token no row of the left table has, at the column's scale.
/// The key tuple's identity: each token as the typed value it reads as
/// (a `dec` at its column's scale, so two spellings of one number agree),
/// or the token itself when it does not parse.
fn canonical_tuple(cols: &[Column], tup: &[String]) -> Vec<String> {
    cols.iter()
        .zip(tup)
        .map(|(c, tok)| {
            let parsed = match c.ty {
                SpecType::Dec => tok
                    .parse::<bigdecimal::BigDecimal>()
                    .ok()
                    .map(|d| d.normalized().to_string()),
                ty => tm_formats::typed_value::TypedValue::from_text(tok, ty)
                    .map(|v| v.to_json().to_string()),
            };
            parsed.unwrap_or_else(|| tok.clone())
        })
        .collect()
}

fn unmatched_token(ty: SpecType, scale: u8, i: usize) -> String {
    match ty {
        SpecType::Str => format!("ZZZ{i}"),
        SpecType::Int => (900 + i).to_string(),
        SpecType::Dec => positional(900 + i as i128, scale),
        _ => format!("1999-12-{:02}", 29 - i),
    }
}

/// The table's shape at one point of a pipeline (11.18.2), with what the
/// generator needs beside it: a value pool per column for `row` and
/// `expected`, which columns hold missing, `nan`, or `inf` cells (a group
/// on one has a missing group), and whether a total order is in force.
#[derive(Clone)]
struct Schema {
    columns: Vec<(String, Tag)>,
    key: Vec<String>,
    pools: BTreeMap<String, Vec<Value>>,
    specials: BTreeSet<String>,
    ordered: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Class {
    Num,
    Str,
    Bool,
    Date,
    DateTime,
}

fn class(t: Tag) -> Class {
    match t {
        Tag::Str => Class::Str,
        Tag::Bool => Class::Bool,
        Tag::Date => Class::Date,
        Tag::DateTime => Class::DateTime,
        _ => Class::Num,
    }
}

impl Schema {
    fn tag(&self, name: &str) -> Option<Tag> {
        self.columns
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, t)| *t)
    }
    fn names(&self) -> Vec<String> {
        self.columns.iter().map(|(n, _)| n.clone()).collect()
    }
    fn of_class(&self, c: Class) -> Vec<String> {
        self.columns
            .iter()
            .filter(|(_, t)| class(*t) == c)
            .map(|(n, _)| n.clone())
            .collect()
    }
    fn has(&self, name: &str) -> bool {
        self.tag(name).is_some()
    }
    fn fresh(&self, base: &str) -> String {
        let mut i = 1;
        loop {
            let n = format!("{base}{i}");
            if !self.has(&n) {
                return n;
            }
            i += 1;
        }
    }
    fn pool(&self, name: &str) -> Vec<Value> {
        self.pools.get(name).cloned().unwrap_or_default()
    }
}

// ------------------------------------------------------- value writing

/// `coef * 10^-scale` in positional notation.
fn positional(coef: i128, scale: u8) -> String {
    let neg = coef < 0;
    let mut digits = coef.unsigned_abs().to_string();
    let scale = usize::from(scale);
    if digits.len() <= scale {
        let pad = scale + 1 - digits.len();
        digits.insert_str(0, &"0".repeat(pad));
    }
    let point = digits.len() - scale;
    let mut out = String::new();
    if neg && coef != 0 {
        out.push('-');
    }
    out.push_str(&digits[..point]);
    if scale > 0 {
        out.push('.');
        out.push_str(&digits[point..]);
    }
    out
}

/// The same value written with an exponent (ruling 32): the mantissa keeps
/// `k` fraction digits (sometimes with trailing zeros, ruling 39) and the
/// exponent makes up the difference.
fn exponent_form(rng: &mut Rng, coef: i128, scale: u8) -> String {
    let digits = coef.unsigned_abs().to_string();
    let extra = if rng.chance(25) { rng.below(3) } else { 0 };
    let k = rng.below(digits.len() + 1) + extra;
    let mut mant = digits.clone();
    mant.push_str(&"0".repeat(extra));
    if mant.len() <= k {
        let pad = k + 1 - mant.len();
        mant.insert_str(0, &"0".repeat(pad));
    }
    let point = mant.len() - k;
    // mantissa = coef * 10^(extra - k); value = coef * 10^-scale.
    let e = k as i64 - i64::from(scale) - extra as i64;
    let mut out = String::new();
    if coef < 0 {
        out.push('-');
    }
    out.push_str(&mant[..point]);
    if k > 0 {
        out.push('.');
        out.push_str(&mant[point..]);
    }
    out.push(if rng.chance(70) { 'e' } else { 'E' });
    if e >= 0 && rng.chance(40) {
        out.push('+');
    }
    let _ = write!(out, "{e}");
    out
}

fn tv_int(n: i128) -> Value {
    json!({"t": "int", "v": n.to_string()})
}
fn tv_dec(text: &str) -> Value {
    json!({"t": "dec", "v": text})
}
fn tv_str(s: &str) -> Value {
    json!({"t": "str", "v": s})
}
fn tv_bool(b: bool) -> Value {
    json!({"t": "bool", "v": b})
}
fn tv_date(s: &str) -> Value {
    json!({"t": "date", "v": s})
}
fn tv_datetime(s: &str) -> Value {
    json!({"t": "datetime", "v": s})
}
fn tv_missing() -> Value {
    json!({"t": "missing"})
}

fn float_token_value(token: &str) -> Option<Value> {
    let x: f64 = token.parse().ok()?;
    if !x.is_finite() {
        return None;
    }
    dec_from_f64_shortest(x).map(|d| tv_dec(&dec_text(&d)))
}

const STR_POOL: [&str; 12] = [
    "a",
    "b",
    "c",
    "liver",
    "lung",
    "breast",
    "caf\u{e9}",
    "cafe\u{301}",
    "x y",
    "Zed",
    "TP53",
    "BRCA1",
];
const KEY_STR_POOL: [&str; 14] = [
    "TP53", "BRCA1", "EGFR", "KRAS", "MDM2", "MYC", "PTEN", "RB1", "APC", "ATM", "s1", "s2", "s3",
    "s4",
];
const DATE_POOL: [&str; 8] = [
    "2020-02-29",
    "2024-01-01",
    "2025-12-31",
    "2026-01-05",
    "2026-03-01",
    "2026-06-30",
    "2027-11-18",
    "2021-07-04",
];
const DATETIME_POOL: [&str; 7] = [
    "2026-01-05T10:00:00Z",
    "2026-01-05T10:00:00.5Z",
    "2026-03-01T00:00:00Z",
    "2025-12-31T23:59:59Z",
    "2025-12-31T23:59:59.999999999Z",
    "2024-02-29T12:30:00.25Z",
    "2020-01-01T00:00:00Z",
];
const FLOAT_POOL: [&str; 14] = [
    "1.5",
    "0.1",
    "0.25",
    "2.5e10",
    "1e-7",
    "123456.789",
    "-3.75",
    "0.30000000000000004",
    "1E3",
    "7",
    "-0.0",
    "0",
    "1.7976931348623157e308",
    "5e-324",
];
const NAN_TOKENS: [&str; 4] = ["NaN", "nan", "NAN", "Nan"];
const INF_TOKENS: [&str; 7] = ["Inf", "-Inf", "inf", "+inf", "-infinity", "Infinity", "INF"];
const COLUMN_NAMES: [&str; 14] = [
    "x", "y", "v", "n", "score", "fc", "p", "grp", "tissue", "d", "ts", "flag", "p-value", "or",
];

/// What a table should look like for a bucket.
#[derive(Clone, Default)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent switches a bucket sets"
)]
struct TableWish {
    /// Columns that must appear, by type; `None` draws freely.
    types: Vec<SpecType>,
    /// Write some `dec` and `float64` cells with an exponent.
    exponents: bool,
    /// Put NaN and Inf tokens in `float64` columns.
    nan_inf: bool,
    /// One finite `float64` cell that overflows a double (ruling 53).
    overflow_cell: bool,
    /// Columns holding missing cells, as a percentage per cell.
    missing_rate: u64,
    /// `dec` columns at scale 33 with values differing in the 34th digit.
    digit34: bool,
    /// Row count bounds.
    rows: (usize, usize),
    /// Key column count bounds.
    key_cols: (usize, usize),
}

impl TableWish {
    fn plain() -> TableWish {
        TableWish {
            missing_rate: 20,
            rows: (2, 12),
            key_cols: (1, 2),
            ..TableWish::default()
        }
    }
}

struct Gen {
    rng: Rng,
    bucket: &'static str,
    tables: Vec<Table>,
}

impl Gen {
    fn used_names(&self) -> BTreeSet<String> {
        self.tables.iter().map(|t| t.name.clone()).collect()
    }

    fn table_name(&mut self) -> String {
        let pool = ["de", "ann", "obs", "sc", "ev", "gp", "meta", "fl", "tiny"];
        let used = self.used_names();
        let free: Vec<&str> = pool
            .iter()
            .copied()
            .filter(|n| !used.contains(*n))
            .collect();
        if free.is_empty() {
            format!("t{}", self.tables.len())
        } else {
            (*self.rng.pick(&free)).to_owned()
        }
    }

    fn column_names(&mut self, n: usize, taken: &BTreeSet<String>) -> Vec<String> {
        let mut free: Vec<String> = COLUMN_NAMES
            .iter()
            .map(|s| (*s).to_owned())
            .filter(|s| !taken.contains(s))
            .collect();
        self.rng.shuffle(&mut free);
        free.truncate(n);
        let mut i = 0;
        while free.len() < n {
            let name = format!("c{i}");
            if !taken.contains(&name) && !free.contains(&name) {
                free.push(name);
            }
            i += 1;
        }
        free
    }

    /// Distinct key tuples for `rows` rows over `types`, as tokens and
    /// typed values per key column.
    fn key_cells(&mut self, types: &[SpecType], rows: usize) -> Vec<(Vec<String>, Vec<Value>)> {
        // A pool per key column, then distinct tuples from the product.
        let pools: Vec<Vec<(String, Value)>> = types
            .iter()
            .enumerate()
            .map(|(j, t)| {
                let want = if types.len() == 1 {
                    rows
                } else if j == 0 {
                    4
                } else {
                    rows.div_ceil(4) + 2
                };
                self.key_pool(*t, want.max(2))
            })
            .collect();
        let mut tuples: Vec<(Vec<String>, Vec<Value>)> = Vec::new();
        if pools.len() == 1 {
            for (tok, v) in &pools[0] {
                tuples.push((vec![tok.clone()], vec![v.clone()]));
            }
        } else {
            for (a, av) in &pools[0] {
                for (b, bv) in &pools[1] {
                    tuples.push((vec![a.clone(), b.clone()], vec![av.clone(), bv.clone()]));
                }
            }
        }
        self.rng.shuffle(&mut tuples);
        tuples.truncate(rows);
        tuples
    }

    fn key_pool(&mut self, t: SpecType, n: usize) -> Vec<(String, Value)> {
        let mut out = Vec::new();
        match t {
            SpecType::Str => {
                let names = self.rng.sample(&KEY_STR_POOL, n);
                for s in names {
                    out.push((s.to_owned(), tv_str(s)));
                }
            }
            SpecType::Int => {
                let start = self.rng.range(-3, 100);
                let mut ks: Vec<i64> = (0..n as i64)
                    .map(|i| start + i * self.rng.range(1, 3))
                    .collect();
                ks.dedup();
                for k in ks {
                    out.push((k.to_string(), tv_int(i128::from(k))));
                }
            }
            SpecType::Dec => {
                let scale = self.rng.range(1, 2) as u8;
                for i in 0..n {
                    let coef = i128::from(self.rng.range(-5, 5)) + (i as i128) * 10 + 1;
                    let text = positional(coef, scale);
                    out.push((text.clone(), tv_dec(&text)));
                }
            }
            _ => {
                let dates = self.rng.sample(&DATE_POOL, n);
                for d in dates {
                    out.push((d.to_owned(), tv_date(d)));
                }
            }
        }
        // Distinct by token.
        let mut seen = BTreeSet::new();
        out.retain(|(tok, _)| seen.insert(tok.clone()));
        out
    }

    /// A non-key cell: token and typed value (`None` for a special).
    #[allow(clippy::too_many_lines, reason = "one arm per spec type and special")]
    fn cell(
        &mut self,
        ty: SpecType,
        scale: u8,
        wish: &TableWish,
        table: &TableFlags,
        domain: &[i128],
    ) -> (String, Option<Value>) {
        if wish.missing_rate > 0 && self.rng.chance(wish.missing_rate) {
            return (self.rng.pick_owned(&table.missing_tokens), None);
        }
        match ty {
            SpecType::Int => {
                let v = self.rng.pick_owned(domain);
                (v.to_string(), Some(tv_int(v)))
            }
            SpecType::Dec => {
                let coef = self.rng.pick_owned(domain);
                let text = positional(coef, scale);
                let token = if wish.exponents && self.rng.chance(50) {
                    exponent_form(&mut self.rng, coef, scale)
                } else {
                    text.clone()
                };
                (token, Some(tv_dec(&text)))
            }
            SpecType::Float64 => {
                if wish.nan_inf && self.rng.chance(30) {
                    let token = if self.rng.chance(50) {
                        self.rng.pick_owned(&NAN_TOKENS)
                    } else {
                        self.rng.pick_owned(&INF_TOKENS)
                    };
                    return (token.to_owned(), None);
                }
                let token = if wish.exponents && self.rng.chance(40) {
                    let mant = self.rng.pick(&["1.5", "2", "7.25", "1.0", "3"]);
                    format!(
                        "{mant}{}{}{}",
                        if self.rng.chance(50) { 'e' } else { 'E' },
                        if self.rng.chance(50) { "-" } else { "" },
                        self.rng.range(0, 12)
                    )
                } else {
                    let d = self.rng.pick_owned(domain);
                    FLOAT_POOL[(d.unsigned_abs() % FLOAT_POOL.len() as u128) as usize].to_owned()
                };
                let v = float_token_value(&token);
                (token, v)
            }
            SpecType::Str => {
                let i =
                    (self.rng.pick_owned(domain).unsigned_abs() % STR_POOL.len() as u128) as usize;
                let s = STR_POOL[i];
                (s.to_owned(), Some(tv_str(&tm_formats::sort_order::nfc(s))))
            }
            SpecType::Bool => {
                let b = self.rng.chance(50);
                (b.to_string(), Some(tv_bool(b)))
            }
            SpecType::Date => {
                let i =
                    (self.rng.pick_owned(domain).unsigned_abs() % DATE_POOL.len() as u128) as usize;
                (DATE_POOL[i].to_owned(), Some(tv_date(DATE_POOL[i])))
            }
            SpecType::DateTime => {
                let i = (self.rng.pick_owned(domain).unsigned_abs() % DATETIME_POOL.len() as u128)
                    as usize;
                (
                    DATETIME_POOL[i].to_owned(),
                    Some(tv_datetime(DATETIME_POOL[i])),
                )
            }
        }
    }

    #[allow(clippy::too_many_lines, reason = "key, then one pass per column")]
    fn table(&mut self, wish: &TableWish) -> Table {
        let name = self.table_name();
        let rows = self.rng.range(wish.rows.0 as i64, wish.rows.1 as i64) as usize;
        let nkey = self
            .rng
            .range(wish.key_cols.0 as i64, wish.key_cols.1 as i64) as usize;
        let key_types: Vec<SpecType> = (0..nkey)
            .map(|_| {
                self.rng.pick_owned(&[
                    SpecType::Str,
                    SpecType::Str,
                    SpecType::Int,
                    SpecType::Dec,
                    SpecType::Date,
                ])
            })
            .collect();
        let tsv = self.rng.chance(20);
        let missing_tokens: Vec<String> = if self.rng.chance(70) {
            vec!["NA".into()]
        } else {
            vec!["NA".into(), String::new()]
        };
        let flags = TableFlags {
            missing_tokens: missing_tokens.clone(),
        };

        // Non-key column types: the wish's, then random ones.
        let mut types = wish.types.clone();
        let extra = self.rng.range(i64::from(types.is_empty()), 3) as usize;
        for _ in 0..extra {
            types.push(self.rng.pick_owned(&[
                SpecType::Int,
                SpecType::Int,
                SpecType::Dec,
                SpecType::Dec,
                SpecType::Str,
                SpecType::Bool,
                SpecType::Date,
                SpecType::DateTime,
                SpecType::Float64,
            ]));
        }
        let mut taken: BTreeSet<String> = BTreeSet::new();
        let key_names: Vec<String> = key_types
            .iter()
            .enumerate()
            .map(|(j, t)| {
                let n = match (nkey, j, t) {
                    (1, _, SpecType::Str) => "gene_id".to_owned(),
                    (1, _, _) => "id".to_owned(),
                    (_, 0, _) => "sample".to_owned(),
                    _ => "gene".to_owned(),
                };
                taken.insert(n.clone());
                n
            })
            .collect();
        let names = self.column_names(types.len(), &taken);

        let keys = self.key_cells(&key_types, rows);
        let rows = keys.len();
        let mut columns: Vec<Column> = Vec::new();
        for (j, kt) in key_types.iter().enumerate() {
            columns.push(Column {
                name: key_names[j].clone(),
                ty: *kt,
                scale: u8::from(*kt == SpecType::Dec),
                tokens: keys.iter().map(|(t, _)| t[j].clone()).collect(),
                values: keys.iter().map(|(_, v)| Some(v[j].clone())).collect(),
            });
        }
        // A key dec column's scale is the largest its cells show.
        for c in &mut columns {
            if c.ty == SpecType::Dec {
                c.scale = c
                    .tokens
                    .iter()
                    .map(|t| t.split('.').nth(1).map_or(0, str::len))
                    .max()
                    .unwrap_or(0) as u8;
            }
        }
        for (j, ty) in types.iter().enumerate() {
            let (scale, domain) = self.domain(*ty, wish);
            let mut tokens = Vec::with_capacity(rows);
            let mut values = Vec::with_capacity(rows);
            for _ in 0..rows {
                let (t, v) = self.cell(*ty, scale, wish, &flags, &domain);
                tokens.push(t);
                values.push(v);
            }
            if wish.overflow_cell && *ty == SpecType::Float64 && j == 0 && rows > 0 {
                let r = self.rng.below(rows);
                tokens[r] = String::from(*self.rng.pick(&["1e400", "-1e309", "2e308"]));
                values[r] = None;
            }
            columns.push(Column {
                name: names[j].clone(),
                ty: *ty,
                scale,
                tokens,
                values,
            });
        }
        // Column order: key columns need not come first.
        if self.rng.chance(30) && columns.len() > nkey {
            let last = columns.len() - 1;
            columns.swap(0, last);
        }
        let key: Vec<usize> = key_names
            .iter()
            .map(|n| columns.iter().position(|c| &c.name == n).unwrap_or(0))
            .collect();
        Table {
            name,
            columns,
            key,
            tsv,
            nan_value: self.rng.chance(50),
            inf_value: self.rng.chance(50),
            missing_tokens,
        }
    }

    /// A column's scale and the coefficient domain its cells draw from.
    fn domain(&mut self, ty: SpecType, wish: &TableWish) -> (u8, Vec<i128>) {
        match ty {
            SpecType::Dec if wish.digit34 => {
                let base: i128 = 10i128.pow(33);
                let mut d = vec![
                    base,
                    base + 1,
                    base + 2,
                    10i128.pow(34) - 1,
                    10i128.pow(34) - 2,
                    5 * base + 5,
                ];
                d.truncate(self.rng.range(2, 6) as usize);
                (33, d)
            }
            SpecType::Dec => {
                let scale = *self.rng.pick(&[0u8, 1, 1, 2, 3, 4, 6, 10]);
                let size = self.rng.range(2, 6) as usize;
                let mut d: Vec<i128> = (0..size)
                    .map(|_| {
                        let mag = *self.rng.pick(&[1i128, 10, 100, 1000, 25]);
                        i128::from(self.rng.range(-20, 40)) * mag
                    })
                    .collect();
                if self.rng.chance(20) {
                    d.push(0);
                }
                (scale, d)
            }
            SpecType::Int => {
                let size = self.rng.range(2, 6) as usize;
                let mut d: Vec<i128> = (0..size)
                    .map(|_| i128::from(self.rng.range(-5, 20)))
                    .collect();
                if self.rng.chance(15) {
                    d.push(123_456_789_012_345_678_901_234_567);
                }
                if self.rng.chance(15) {
                    d.push(0);
                }
                (0, d)
            }
            _ => {
                let size = self.rng.range(2, 5) as usize;
                let d: Vec<i128> = (0..size)
                    .map(|_| i128::from(self.rng.range(0, 1000)))
                    .collect();
                (0, d)
            }
        }
    }
}

struct TableFlags {
    missing_tokens: Vec<String>,
}

// -------------------------------------------------------- expressions

enum E {
    Col(String),
    Int(String),
    Dec(String),
    Exp(String),
    Str(String),
    Bool(bool),
    Missing,
    Neg(Box<E>),
    Not(Box<E>),
    Bin(&'static str, Box<E>, Box<E>),
    Call(&'static str, Vec<E>),
}

fn level(e: &E) -> u8 {
    match e {
        E::Bin("or", ..) => 1,
        E::Bin("and", ..) => 2,
        E::Not(_) => 3,
        E::Bin("=" | "!=" | "<" | "<=" | ">" | ">=", ..) => 4,
        E::Bin("+" | "-", ..) => 5,
        E::Bin(..) => 6,
        E::Neg(_) => 7,
        _ => 8,
    }
}

fn is_ident(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

const RESERVED: [&str; 6] = ["and", "or", "not", "true", "false", "missing"];

fn col_text(rng: &mut Rng, name: &str) -> String {
    if is_ident(name) && !RESERVED.contains(&name) && !rng.chance(20) {
        name.to_owned()
    } else {
        format!("`{name}`")
    }
}

fn sp(rng: &mut Rng) -> &'static str {
    rng.pick(&["", " ", " ", "  "])
}

fn render(e: &E, rng: &mut Rng) -> String {
    let mut s = render_inner(e, rng);
    if rng.chance(10) {
        s = format!("({s})");
    }
    s
}

fn child(e: &E, needs_parens: bool, rng: &mut Rng) -> String {
    let s = render_inner(e, rng);
    if needs_parens || rng.chance(15) {
        format!("({}{s}{})", sp(rng), sp(rng))
    } else {
        s
    }
}

fn render_inner(e: &E, rng: &mut Rng) -> String {
    match e {
        E::Col(n) => col_text(rng, n),
        E::Int(t) | E::Dec(t) | E::Exp(t) => t.clone(),
        E::Str(s) => {
            let mut out = String::from("\"");
            for ch in s.chars() {
                match ch {
                    '"' => out.push_str("\\\""),
                    '\\' => out.push_str("\\\\"),
                    c => out.push(c),
                }
            }
            out.push('"');
            out
        }
        E::Bool(b) => b.to_string(),
        E::Missing => "missing".into(),
        E::Neg(x) => {
            let inner = child(x, level(x) < 7, rng);
            if inner.starts_with('-') {
                format!("- {inner}")
            } else {
                format!("-{inner}")
            }
        }
        E::Not(x) => format!("not {}", child(x, level(x) < 3, rng)),
        E::Bin(op, a, b) => {
            let l = level(e);
            let left = child(a, level(a) < l || (l == 4 && level(a) == 4), rng);
            let right = child(b, level(b) <= l, rng);
            let keyword = matches!(*op, "and" | "or");
            let s1 = if keyword { " " } else { sp(rng) };
            let s2 = if keyword { " " } else { sp(rng) };
            format!("{left}{s1}{op}{s2}{right}")
        }
        E::Call(f, args) => {
            let parts: Vec<String> = args.iter().map(|a| render_inner(a, rng)).collect();
            let sep = if rng.chance(50) { ", " } else { "," };
            format!("{f}({})", parts.join(sep))
        }
    }
}

/// What an expression draw may use.
#[derive(Clone, Copy, Default)]
struct ExprWish {
    exponents: bool,
    out_of_range: bool,
    log_family: bool,
}

impl Gen {
    fn num_literal(&mut self, wish: ExprWish) -> E {
        if wish.out_of_range {
            return E::Exp(
                (*self
                    .rng
                    .pick(&["1e1000000", "1e-1000000", "9e1000001", "1.5E+1000000"]))
                .to_owned(),
            );
        }
        if wish.exponents && self.rng.chance(50) {
            let mant = *self.rng.pick(&["1", "1.5", "2.50", "7", "0.5", "12.5"]);
            let e = self.rng.range(-12, 12);
            let sign = if e >= 0 && self.rng.chance(30) {
                "+"
            } else {
                ""
            };
            return E::Exp(format!(
                "{mant}{}{sign}{e}",
                if self.rng.chance(60) { "e" } else { "E" }
            ));
        }
        if self.rng.chance(50) {
            E::Int(
                (*self
                    .rng
                    .pick(&["0", "1", "2", "3", "5", "10", "100", "1000000"]))
                .to_owned(),
            )
        } else {
            E::Dec(
                (*self
                    .rng
                    .pick(&["0.5", "1.0", "2.50", "0.001", "3.14159", "0.1", "12.25"]))
                .to_owned(),
            )
        }
    }

    fn num_expr(&mut self, s: &Schema, depth: u8, wish: ExprWish) -> E {
        let cols = s.of_class(Class::Num);
        if depth == 0 || self.rng.chance(35) {
            return if !cols.is_empty() && self.rng.chance(70) {
                E::Col(self.rng.pick_owned(&cols))
            } else {
                self.num_literal(wish)
            };
        }
        match self.rng.below(if wish.log_family { 7 } else { 5 }) {
            0 => E::Neg(Box::new(self.num_expr(s, depth - 1, wish))),
            1 => E::Call("abs", vec![self.num_expr(s, depth - 1, wish)]),
            2 => {
                let n = self.rng.range(2, 3) as usize;
                let args = (0..n).map(|_| self.num_expr(s, depth - 1, wish)).collect();
                E::Call("coalesce", args)
            }
            3 | 4 => {
                let op = *self.rng.pick(&["+", "-", "*", "/"]);
                E::Bin(
                    op,
                    Box::new(self.num_expr(s, depth - 1, wish)),
                    Box::new(self.num_expr(s, depth - 1, wish)),
                )
            }
            _ => {
                let f = *self.rng.pick(&["log", "log2", "log10", "exp"]);
                E::Call(f, vec![self.num_expr(s, depth - 1, wish)])
            }
        }
    }

    fn cmp_expr(&mut self, s: &Schema, depth: u8, wish: ExprWish) -> E {
        let mut classes: Vec<Class> = [
            Class::Num,
            Class::Str,
            Class::Bool,
            Class::Date,
            Class::DateTime,
        ]
        .into_iter()
        .filter(|c| !s.of_class(*c).is_empty())
        .collect();
        if classes.is_empty() || wish.out_of_range {
            classes.push(Class::Num);
        }
        let c = *self.rng.pick(&classes);
        let cols = s.of_class(c);
        let all6: [&'static str; 6] = ["=", "!=", "<", "<=", ">", ">="];
        match c {
            Class::Num => {
                let op = *self.rng.pick(&all6);
                let a = self.num_expr(s, depth, wish);
                let b = if self.rng.chance(10) {
                    E::Missing
                } else {
                    self.num_expr(s, depth, wish)
                };
                E::Bin(op, Box::new(a), Box::new(b))
            }
            Class::Str => {
                let op = if self.rng.chance(70) {
                    *self.rng.pick(&["=", "!="])
                } else {
                    *self.rng.pick(&all6)
                };
                let col = self.rng.pick_owned(&cols);
                let pool = s.pool(&col);
                let rhs = if self.rng.chance(30) && cols.len() > 1 {
                    E::Col(self.rng.pick_owned(&cols))
                } else if !pool.is_empty() && self.rng.chance(70) {
                    let v = self.rng.pick_owned(&pool);
                    E::Str(v["v"].as_str().unwrap_or("a").to_owned())
                } else {
                    E::Str(
                        (*self.rng.pick(&[
                            "liver",
                            "caf\u{e9}",
                            "cafe\u{301}",
                            "say \"hi\"",
                            "back\\slash",
                            "",
                        ]))
                        .to_owned(),
                    )
                };
                let lhs = if self.rng.chance(15) {
                    E::Call("coalesce", vec![E::Col(col), E::Str("none".into())])
                } else {
                    E::Col(col)
                };
                E::Bin(op, Box::new(lhs), Box::new(rhs))
            }
            Class::Bool => {
                let col = self.rng.pick_owned(&cols);
                if self.rng.chance(40) {
                    return E::Col(col);
                }
                let op = *self.rng.pick(&["=", "!="]);
                E::Bin(
                    op,
                    Box::new(E::Col(col)),
                    Box::new(E::Bool(self.rng.chance(50))),
                )
            }
            Class::Date | Class::DateTime => {
                let op = *self.rng.pick(&all6);
                let a = self.rng.pick_owned(&cols);
                let b = self.rng.pick_owned(&cols);
                let rhs = if self.rng.chance(20) {
                    E::Call("coalesce", vec![E::Col(b.clone()), E::Col(a.clone())])
                } else {
                    E::Col(b)
                };
                E::Bin(op, Box::new(E::Col(a)), Box::new(rhs))
            }
        }
    }

    fn bool_expr(&mut self, s: &Schema, depth: u8, wish: ExprWish) -> E {
        if depth == 0 || self.rng.chance(45) {
            return match self.rng.below(20) {
                0 => E::Bool(true),
                1 => E::Bool(false),
                _ => self.cmp_expr(s, depth.saturating_sub(1), wish),
            };
        }
        match self.rng.below(4) {
            0 => E::Not(Box::new(self.bool_expr(s, depth - 1, wish))),
            1 | 2 => {
                let op = *self.rng.pick(&["and", "or"]);
                E::Bin(
                    op,
                    Box::new(self.bool_expr(s, depth - 1, wish)),
                    Box::new(self.bool_expr(s, depth - 1, wish)),
                )
            }
            _ => self.cmp_expr(s, depth - 1, wish),
        }
    }
}

// ----------------------------------------------------------------- ops

struct Pipeline {
    ops: Vec<Value>,
    schema: Schema,
}

impl Gen {
    fn op_filter(&mut self, p: &mut Pipeline, wish: ExprWish) {
        let e = self.bool_expr(&p.schema, 2, wish);
        let text = render(&e, &mut self.rng);
        p.ops.push(json!({"op": "filter", "v": 1, "where": text}));
    }

    fn op_derive(&mut self, p: &mut Pipeline, wish: ExprWish) {
        let name = p.schema.fresh("d");
        let (e, tag) = match self.rng.below(10) {
            0 => (self.bool_expr(&p.schema, 2, wish), Tag::Bool),
            1 if !p.schema.of_class(Class::Str).is_empty() => {
                let col = self.rng.pick_owned(&p.schema.of_class(Class::Str));
                (
                    E::Call("coalesce", vec![E::Col(col), E::Str("none".into())]),
                    Tag::Str,
                )
            }
            _ => (self.num_expr(&p.schema, 2, wish), Tag::Dec),
        };
        let text = render(&e, &mut self.rng);
        p.ops
            .push(json!({"op": "derive", "v": 1, "as": name, "expr": text}));
        p.schema.columns.push((name, tag));
    }

    /// `coalesce(dec, int)` over two columns, so one column holds equal
    /// values written differently (ruling 51) and `int` and `dec` members
    /// fall in one group under `=`.
    fn op_derive_coalesce_widen(&mut self, p: &mut Pipeline) -> Option<String> {
        let decs: Vec<String> = p
            .schema
            .columns
            .iter()
            .filter(|(_, t)| *t == Tag::Dec)
            .map(|(n, _)| n.clone())
            .collect();
        let ints: Vec<String> = p
            .schema
            .columns
            .iter()
            .filter(|(_, t)| *t == Tag::Int)
            .map(|(n, _)| n.clone())
            .collect();
        if decs.is_empty() || ints.is_empty() {
            return None;
        }
        let name = p.schema.fresh("m");
        let (a, b) = if self.rng.chance(50) {
            (self.rng.pick_owned(&decs), self.rng.pick_owned(&ints))
        } else {
            (self.rng.pick_owned(&ints), self.rng.pick_owned(&decs))
        };
        let e = E::Call("coalesce", vec![E::Col(a), E::Col(b)]);
        let text = render(&e, &mut self.rng);
        p.ops
            .push(json!({"op": "derive", "v": 1, "as": name, "expr": text}));
        p.schema.columns.push((name.clone(), Tag::Dec));
        Some(name)
    }

    fn op_select(&mut self, p: &mut Pipeline) {
        let names = p.schema.names();
        let n = self.rng.range(1, names.len() as i64) as usize;
        let mut cols = self.rng.sample(&names, n);
        if self.rng.chance(30) {
            // A key column written explicitly (ruling 49).
            let k = self.rng.pick_owned(&p.schema.key);
            if !cols.contains(&k) {
                cols.push(k);
            }
        }
        p.ops.push(json!({"op": "select", "v": 1, "columns": cols}));
        let key = p.schema.key.clone();
        p.schema
            .columns
            .retain(|(n, _)| cols.contains(n) || key.contains(n));
    }

    fn op_sort(&mut self, p: &mut Pipeline) {
        let names = p.schema.names();
        let n = self.rng.range(1, 3_i64.min(names.len() as i64)) as usize;
        let cols = self.rng.sample(&names, n);
        let by: Vec<Value> = cols
            .iter()
            .map(|c| json!({"column": c, "dir": if self.rng.chance(50) { "asc" } else { "desc" }}))
            .collect();
        let missing = if self.rng.chance(50) { "first" } else { "last" };
        p.ops
            .push(json!({"op": "sort", "v": 1, "by": by, "missing": missing}));
        p.schema.ordered = true;
    }

    fn op_take(&mut self, p: &mut Pipeline) {
        let n = self.rng.range(0, 13);
        p.ops.push(json!({"op": "take", "v": 1, "n": n}));
    }

    fn op_rank(&mut self, p: &mut Pipeline, group: Option<Vec<String>>) {
        let names = p.schema.names();
        let by = self.rng.pick_owned(&names);
        let ties = *self.rng.pick(&["min", "max", "dense", "ordinal"]);
        let missing = *self.rng.pick(&["first", "last", "exclude"]);
        let dir = if self.rng.chance(50) { "asc" } else { "desc" };
        let mut op = json!({"op": "rank", "v": 1, "by": by, "dir": dir, "ties": ties, "missing": missing, "scope": "all"});
        if let Some(g) = group
            && let Some(m) = op.as_object_mut()
        {
            m.insert("scope".into(), json!("group"));
            m.insert("group_by".into(), json!(g));
        }
        p.ops.push(op);
        p.schema.columns.push(("rank".into(), Tag::Int));
        p.schema.ordered = true;
    }

    fn agg_for(&mut self, tag: Tag) -> &'static str {
        match class(tag) {
            Class::Num => self
                .rng
                .pick(&["count", "sum", "mean", "min", "max", "median"]),
            Class::Bool => "count",
            _ => self.rng.pick(&["count", "min", "max"]),
        }
    }

    fn op_group(&mut self, p: &mut Pipeline, by: &[String], prefer: Option<&str>) {
        let names = p.schema.names();
        let n = self.rng.range(1, 3) as usize;
        let mut aggs = Vec::new();
        let mut out = Schema {
            columns: Vec::new(),
            key: by.to_vec(),
            pools: BTreeMap::new(),
            specials: BTreeSet::new(),
            ordered: false,
        };
        for b in by {
            out.columns
                .push((b.clone(), p.schema.tag(b).unwrap_or(Tag::Str)));
            out.pools.insert(b.clone(), p.schema.pool(b));
            if p.schema.specials.contains(b) {
                out.specials.insert(b.clone());
            }
        }
        for i in 0..n {
            let col = match (prefer, i) {
                (Some(m), 0) => m.to_owned(),
                _ => self.rng.pick_owned(&names),
            };
            let tag = p.schema.tag(&col).unwrap_or(Tag::Int);
            let func = if prefer.is_some() && i == 0 {
                *self.rng.pick(&["min", "max"])
            } else {
                self.agg_for(tag)
            };
            let as_ = format!("a{}", i + 1);
            let out_tag = match func {
                "count" => Tag::Int,
                "mean" | "median" => Tag::Dec,
                _ => tag,
            };
            aggs.push(json!({"as": as_, "fn": func, "column": col}));
            out.columns.push((as_, out_tag));
        }
        out.columns.push(("member_count".into(), Tag::Int));
        out.columns.push(("member_keys_hash".into(), Tag::Str));
        p.ops
            .push(json!({"op": "group", "v": 1, "by": by, "aggregates": aggs}));
        p.schema = out;
    }

    /// A right table whose key the left table can join on, registered in
    /// `self.tables`; returns the `on` pairs.
    #[allow(
        clippy::too_many_lines,
        reason = "the right key, then its other columns"
    )]
    fn op_join(&mut self, p: &mut Pipeline, left: &Table) {
        // Right key: one or two columns typed like left columns.
        let candidates: Vec<&Column> = left
            .columns
            .iter()
            .filter(|c| {
                matches!(
                    c.ty,
                    SpecType::Str | SpecType::Int | SpecType::Dec | SpecType::Date
                )
            })
            .collect();
        let use_key = candidates.is_empty() || self.rng.chance(60);
        let left_cols: Vec<Column> = if use_key {
            left.key.iter().map(|&i| left.columns[i].clone()).collect()
        } else {
            vec![(*self.rng.pick(&candidates)).clone()]
        };
        let rname = self.table_name();
        let rows = self.rng.range(2, 8) as usize;
        // Right key cells: drawn from the left columns' present values so
        // that matches happen, plus a few unmatched ones.
        // Distinct by value, not by spelling: `-150E-10` and `-0.15000E-7`
        // are one key (`E3101` otherwise).
        let mut tuples: BTreeMap<Vec<String>, Vec<String>> = BTreeMap::new();
        let left_rows = left.rows();
        for _ in 0..rows * 2 {
            if tuples.len() >= rows {
                break;
            }
            let r = self.rng.below(left_rows.max(1));
            let mut tup = Vec::new();
            let mut ok = true;
            for c in &left_cols {
                if c.values.get(r).is_none_or(Option::is_none) {
                    ok = false;
                    break;
                }
                tup.push(c.tokens[r].clone());
            }
            if ok {
                tuples
                    .entry(canonical_tuple(&left_cols, &tup))
                    .or_insert(tup);
            }
        }
        let unmatched = self.rng.range(0, 2) as usize;
        for i in 0..unmatched {
            let mut tup = vec![unmatched_token(left_cols[0].ty, left_cols[0].scale, i)];
            for c in &left_cols[1..] {
                tup.push(c.tokens[0].clone());
            }
            tuples
                .entry(canonical_tuple(&left_cols, &tup))
                .or_insert(tup);
        }
        let tuples: Vec<Vec<String>> = tuples.into_values().collect();
        let right_rows = tuples.len();
        let taken: BTreeSet<String> = left_cols.iter().map(|c| c.name.clone()).collect();
        let rkey_names: Vec<String> = left_cols
            .iter()
            .map(|c| {
                if self.rng.chance(60) {
                    c.name.clone()
                } else {
                    format!("r_{}", c.name)
                }
            })
            .collect();
        let mut columns: Vec<Column> = Vec::new();
        for (j, c) in left_cols.iter().enumerate() {
            columns.push(Column {
                name: rkey_names[j].clone(),
                ty: c.ty,
                scale: c.scale,
                tokens: tuples.iter().map(|t| t[j].clone()).collect(),
                values: tuples
                    .iter()
                    .map(|t| {
                        tm_formats::typed_value::TypedValue::from_text(&t[j], c.ty)
                            .map(|v| v.to_json())
                    })
                    .collect(),
            });
        }
        let wish = TableWish::plain();
        let ntypes = self.rng.range(1, 2) as usize;
        let names = self.column_names(ntypes, &taken);
        let flags = TableFlags {
            missing_tokens: vec!["NA".into()],
        };
        for name in &names {
            let ty = self.rng.pick_owned(&[
                SpecType::Int,
                SpecType::Dec,
                SpecType::Str,
                SpecType::Bool,
                SpecType::Date,
            ]);
            let (scale, domain) = self.domain(ty, &wish);
            let mut tokens = Vec::new();
            let mut values = Vec::new();
            for _ in 0..right_rows {
                let (t, v) = self.cell(ty, scale, &wish, &flags, &domain);
                tokens.push(t);
                values.push(v);
            }
            columns.push(Column {
                name: name.clone(),
                ty,
                scale,
                tokens,
                values,
            });
        }
        let right = Table {
            name: rname.clone(),
            key: (0..left_cols.len()).collect(),
            columns,
            tsv: self.rng.chance(20),
            nan_value: true,
            inf_value: true,
            missing_tokens: vec!["NA".into()],
        };
        let on: Vec<Value> = left_cols
            .iter()
            .zip(&rkey_names)
            .map(|(l, r)| json!({"left": l.name, "right": r}))
            .collect();
        let kind = if self.rng.chance(50) { "inner" } else { "left" };
        p.ops
            .push(json!({"op": "join", "v": 1, "right": rname, "on": on, "kind": kind}));
        for c in &right.columns {
            let full = format!("{rname}.{}", c.name);
            p.schema.columns.push((full.clone(), c.ty.tag()));
            p.schema.pools.insert(full.clone(), c.present());
            if c.has_specials() || kind == "left" {
                p.schema.specials.insert(full);
            }
        }
        self.tables.push(right);
    }

    /// Between `lo` and `hi` distinct column names of the schema.
    fn some_names(&mut self, s: &Schema, lo: i64, hi: i64) -> Vec<String> {
        let names = s.names();
        let k = self.rng.range(lo, hi) as usize;
        self.rng.sample(&names, k)
    }

    /// A random op that fits the current schema (the `pipeline` bucket).
    fn op_random(&mut self, p: &mut Pipeline, left: &Table, wish: ExprWish) {
        let joined = p.ops.iter().any(|o| o["op"] == "join");
        let grouped = p.ops.iter().any(|o| o["op"] == "group");
        match self.rng.below(9) {
            0 | 1 => self.op_filter(p, wish),
            2 => self.op_derive(p, wish),
            3 => self.op_select(p),
            4 => {
                self.op_sort(p);
                if self.rng.chance(50) {
                    self.op_take(p);
                }
            }
            5 if !p.schema.has("rank") => self.op_rank(p, None),
            6 if !p.schema.has("rank") => {
                let g = self.some_names(&p.schema, 1, 2);
                self.op_rank(p, Some(g));
            }
            7 if !grouped => {
                let by = self.some_names(&p.schema, 1, 2);
                self.op_group(p, &by, None);
            }
            8 if !joined && !grouped => self.op_join(p, left),
            _ if p.schema.ordered => self.op_take(p),
            _ => self.op_filter(p, wish),
        }
    }
}

// ------------------------------------------------------------ assertion

impl Gen {
    fn absent_value(&mut self, tag: Tag) -> Value {
        match class(tag) {
            Class::Num => {
                if tag == Tag::Int || self.rng.chance(50) {
                    tv_int(i128::from(self.rng.range(900, 999)))
                } else {
                    tv_dec(&format!(
                        "{}.{}",
                        self.rng.range(900, 999),
                        self.rng.range(0, 9)
                    ))
                }
            }
            Class::Str => tv_str(self.rng.pick(&["ZZZ3", "nobody", ""])),
            Class::Bool => tv_bool(self.rng.chance(50)),
            Class::Date => tv_date("1999-12-31"),
            Class::DateTime => tv_datetime("1999-12-31T00:00:00Z"),
        }
    }

    /// A value of the column's type: from its pool mostly, else absent.
    fn value_for(&mut self, s: &Schema, col: &str) -> Value {
        let tag = s.tag(col).unwrap_or(Tag::Str);
        let pool = s.pool(col);
        if !pool.is_empty() && self.rng.chance(75) {
            let v = self.rng.pick_owned(&pool);
            // Say an int as a dec or a dec as an int now and then.
            if tag == Tag::Int && self.rng.chance(20) {
                return tv_dec(&format!("{}.0", v["v"].as_str().unwrap_or("0")));
            }
            return v;
        }
        if tag == Tag::Dec && self.rng.chance(30) {
            return tv_int(i128::from(self.rng.range(-5, 20)));
        }
        self.absent_value(tag)
    }

    /// A `row`: one value per key column; `missing` names the missing group
    /// (ruling 55) where a group `by` column held specials.
    fn row(&mut self, s: &Schema, force_missing: bool) -> Vec<Value> {
        let key = s.key.clone();
        let mut row = Vec::new();
        for k in &key {
            let can_miss = s.specials.contains(k) && s.pools.contains_key(k);
            if (can_miss && (force_missing || self.rng.chance(30))) || self.rng.chance(5) {
                row.push(tv_missing());
            } else {
                row.push(self.value_for(s, k));
            }
        }
        row
    }

    fn tol(&mut self) -> Value {
        let abs = *self.rng.pick(&["0", "0.5", "0.001", "1", "0.000000001"]);
        let rel = *self.rng.pick(&["0", "0", "0.1", "0.2", "0.0000001"]);
        json!({"abs": abs, "rel": rel})
    }

    fn op_for(&mut self, tag: Tag) -> &'static str {
        let all7 = ["eq", "ne", "lt", "le", "gt", "ge", "between"];
        match class(tag) {
            Class::Num | Class::Date | Class::DateTime => self.rng.pick(&all7),
            _ if self.rng.chance(5) => self.rng.pick(&all7),
            _ => self.rng.pick(&["eq", "ne"]),
        }
    }

    fn expected_for(&mut self, s: &Schema, col: &str, op: &str) -> Value {
        if op == "between" {
            let mut lo = self.value_for(s, col);
            let mut hi = self.value_for(s, col);
            let reversed = matches!(
                (lo["v"].as_str(), hi["v"].as_str()),
                (Some(first), Some(second)) if first > second
            );
            if reversed && self.rng.chance(85) {
                std::mem::swap(&mut lo, &mut hi);
            }
            return json!([lo, hi]);
        }
        self.value_for(s, col)
    }

    fn assertion(&mut self, s: &Schema, force_missing: bool) -> Value {
        let names = s.names();
        let kinds = [
            "value",
            "value",
            "value",
            "count",
            "rank",
            "exists",
            "compare_rows",
            "compare_rows",
        ];
        let kind = *self.rng.pick(&kinds);
        match kind {
            "count" => {
                let op = *self
                    .rng
                    .pick(&["eq", "ne", "lt", "le", "gt", "ge", "between"]);
                let expected = if op == "between" {
                    let a = self.rng.range(0, 6);
                    let b = a + self.rng.range(0, 4);
                    json!([tv_int(i128::from(a)), tv_int(i128::from(b))])
                } else {
                    tv_int(i128::from(self.rng.range(0, 13)))
                };
                json!({"assert": 1, "kind": "count", "op": op, "expected": expected})
            }
            "rank" => {
                let row = self.row(s, force_missing);
                let mut a = json!({"assert": 1, "kind": "rank", "row": row, "expected": tv_int(i128::from(self.rng.range(1, 8)))});
                if self.rng.chance(25)
                    && let Some(m) = a.as_object_mut()
                {
                    m.insert("tol".into(), json!({"abs": "1", "rel": "0"}));
                }
                a
            }
            "exists" => {
                let row = self.row(s, force_missing);
                json!({"assert": 1, "kind": "exists", "row": row})
            }
            "compare_rows" => {
                let lcol = self.rng.pick_owned(&names);
                let ltag = s.tag(&lcol).unwrap_or(Tag::Str);
                let same: Vec<String> = s
                    .columns
                    .iter()
                    .filter(|(_, t)| class(*t) == class(ltag))
                    .map(|(n, _)| n.clone())
                    .collect();
                let rcol = self.rng.pick_owned(&same);
                // `str` and `bool` take only `eq` and `ne` (11.18.3); no
                // `between` here.
                let op = match self.op_for(ltag) {
                    "between" => "le",
                    op => op,
                };
                let left = json!({"row": self.row(s, force_missing), "column": lcol});
                let right = json!({"row": self.row(s, false), "column": rcol});
                let mut a = json!({"assert": 1, "kind": "compare_rows", "left": left, "right": right, "op": op});
                if class(ltag) == Class::Num
                    && self.rng.chance(35)
                    && let Some(m) = a.as_object_mut()
                {
                    m.insert("tol".into(), self.tol());
                }
                a
            }
            _ => {
                let col = self.rng.pick_owned(&names);
                let tag = s.tag(&col).unwrap_or(Tag::Str);
                let op = self.op_for(tag);
                let row = self.row(s, force_missing);
                let expected = self.expected_for(s, &col, op);
                let mut a = json!({"assert": 1, "kind": "value", "row": row, "column": col, "op": op, "expected": expected});
                if class(tag) == Class::Num
                    && self.rng.chance(40)
                    && let Some(m) = a.as_object_mut()
                {
                    m.insert("tol".into(), self.tol());
                }
                a
            }
        }
    }
}

// -------------------------------------------------------------- buckets

struct Case {
    tables: Vec<Table>,
    steps: Value,
    assertion: Value,
}

impl Gen {
    fn start(&mut self, wish: &TableWish) -> (Table, Pipeline) {
        let t = self.table(wish);
        let p = Pipeline {
            ops: Vec::new(),
            schema: t.schema(),
        };
        self.tables.push(t.clone());
        (t, p)
    }

    fn finish(&mut self, from: &Table, p: &Pipeline, force_missing: bool) -> Case {
        let mut tables = serde_json::Map::new();
        for (i, t) in self.tables.iter().enumerate() {
            tables.insert(t.name.clone(), json!(format!("F{}@v1", i + 1)));
        }
        let steps = json!({"steps": 1, "tables": tables, "from": from.name, "ops": p.ops});
        let assertion = self.assertion(&p.schema, force_missing);
        Case {
            tables: std::mem::take(&mut self.tables),
            steps,
            assertion,
        }
    }

    /// Columns of the start table that hold a special, as group candidates.
    fn special_columns(t: &Table) -> Vec<String> {
        t.columns
            .iter()
            .filter(|c| c.has_specials())
            .map(|c| c.name.clone())
            .collect()
    }

    #[allow(clippy::too_many_lines, reason = "one arm per bucket")]
    fn case(&mut self) -> Case {
        let plain = TableWish::plain();
        let ew = ExprWish::default();
        match self.bucket {
            "filter" => {
                let (t, mut p) = self.start(&plain);
                self.op_filter(&mut p, ew);
                if self.rng.chance(25) {
                    self.op_filter(&mut p, ew);
                }
                self.finish(&t, &p, false)
            }
            "derive" => {
                let wish = TableWish {
                    types: vec![SpecType::Int, SpecType::Dec],
                    ..plain
                };
                let (t, mut p) = self.start(&wish);
                self.op_derive(&mut p, ew);
                if self.rng.chance(40) {
                    self.op_derive(&mut p, ew);
                }
                if self.rng.chance(40) {
                    self.op_filter(&mut p, ew);
                }
                self.finish(&t, &p, false)
            }
            "logexp" => {
                let wish = TableWish {
                    types: vec![SpecType::Dec, SpecType::Int],
                    ..plain
                };
                let (t, mut p) = self.start(&wish);
                let lw = ExprWish {
                    log_family: true,
                    ..ew
                };
                let name = p.schema.fresh("l");
                let f = *self.rng.pick(&["log", "log2", "log10", "exp"]);
                let arg = self.num_expr(&p.schema, 1, lw);
                let text = render(&E::Call(f, vec![arg]), &mut self.rng);
                p.ops
                    .push(json!({"op": "derive", "v": 1, "as": name, "expr": text}));
                p.schema.columns.push((name, Tag::Dec));
                if self.rng.chance(40) {
                    self.op_filter(&mut p, lw);
                }
                if self.rng.chance(30) {
                    self.op_sort(&mut p);
                }
                self.finish(&t, &p, false)
            }
            "sort-take" => {
                let (t, mut p) = self.start(&plain);
                self.op_sort(&mut p);
                if self.rng.chance(70) {
                    self.op_take(&mut p);
                }
                self.finish(&t, &p, false)
            }
            "rank-all" => {
                let (t, mut p) = self.start(&plain);
                self.op_rank(&mut p, None);
                match self.rng.below(4) {
                    0 => self.op_take(&mut p),
                    1 => {
                        let n = self.rng.range(1, 4);
                        p.ops
                            .push(json!({"op": "filter", "v": 1, "where": format!("rank <= {n}")}));
                    }
                    _ => {}
                }
                self.finish(&t, &p, false)
            }
            "rank-group" => {
                let wish = TableWish {
                    types: vec![SpecType::Float64, SpecType::Int],
                    nan_inf: true,
                    missing_rate: 30,
                    rows: (4, 12),
                    ..plain
                };
                let (t, mut p) = self.start(&wish);
                let names = p.schema.names();
                let mut g = vec![t.first_of(SpecType::Float64)];
                if self.rng.chance(30) {
                    g.push(self.rng.pick_owned(&names));
                    g.dedup();
                }
                self.op_rank(&mut p, Some(g));
                self.finish(&t, &p, false)
            }
            "group" => {
                let wish = TableWish {
                    types: vec![SpecType::Dec, SpecType::Int, SpecType::Str],
                    nan_inf: true,
                    missing_rate: 25,
                    ..plain
                };
                let (t, mut p) = self.start(&wish);
                let prefer = if self.rng.chance(40) {
                    self.op_derive_coalesce_widen(&mut p)
                } else {
                    None
                };
                let by = if prefer.is_some() && self.rng.chance(30) {
                    prefer.clone().into_iter().collect()
                } else {
                    self.some_names(&p.schema, 1, 2)
                };
                self.op_group(&mut p, &by, prefer.as_deref());
                if self.rng.chance(25) {
                    self.op_sort(&mut p);
                }
                self.finish(&t, &p, false)
            }
            "join" => {
                let (t, mut p) = self.start(&plain);
                self.op_join(&mut p, &t);
                match self.rng.below(4) {
                    0 => self.op_filter(&mut p, ew),
                    1 => self.op_select(&mut p),
                    2 => self.op_sort(&mut p),
                    _ => {}
                }
                self.finish(&t, &p, false)
            }
            "select" => {
                let (t, mut p) = self.start(&plain);
                if self.rng.chance(40) {
                    self.op_filter(&mut p, ew);
                }
                self.op_select(&mut p);
                if self.rng.chance(40) {
                    self.op_sort(&mut p);
                }
                self.finish(&t, &p, false)
            }
            "exponent" => {
                let wish = TableWish {
                    types: vec![SpecType::Dec, SpecType::Float64],
                    exponents: true,
                    ..plain
                };
                let (t, mut p) = self.start(&wish);
                let xw = ExprWish {
                    exponents: true,
                    out_of_range: self.rng.chance(15),
                    ..ew
                };
                if self.rng.chance(50) {
                    self.op_filter(&mut p, xw);
                } else {
                    self.op_derive(&mut p, xw);
                }
                self.finish(&t, &p, false)
            }
            "float64" => {
                let wish = TableWish {
                    types: vec![SpecType::Float64, SpecType::Float64],
                    nan_inf: true,
                    exponents: self.rng.chance(40),
                    overflow_cell: self.rng.chance(15),
                    missing_rate: 15,
                    ..plain
                };
                let (t, mut p) = self.start(&wish);
                match self.rng.below(5) {
                    0 => self.op_filter(&mut p, ew),
                    1 => self.op_sort(&mut p),
                    2 => self.op_rank(&mut p, None),
                    3 => {
                        let by = vec![t.first_of(SpecType::Float64)];
                        self.op_group(&mut p, &by, None);
                    }
                    _ => self.op_derive(&mut p, ew),
                }
                self.finish(&t, &p, false)
            }
            "digit34" => {
                let wish = TableWish {
                    types: vec![SpecType::Dec],
                    digit34: true,
                    missing_rate: 10,
                    ..plain
                };
                let (t, mut p) = self.start(&wish);
                let x = t.first_of(SpecType::Dec);
                let xt = col_text(&mut self.rng, &x);
                match self.rng.below(5) {
                    0 => {
                        let lit = *self.rng.pick(&[
                            "1.000000000000000000000000000000001",
                            "1.000000000000000000000000000000002",
                            "9.999999999999999999999999999999999",
                            "1e-33",
                        ]);
                        let op = *self.rng.pick(&["<", "<=", ">", ">=", "=", "!="]);
                        p.ops.push(
                            json!({"op": "filter", "v": 1, "where": format!("{xt} {op} {lit}")}),
                        );
                    }
                    1 => self.op_sort(&mut p),
                    2 => {
                        let e = *self.rng.pick(&[
                            "{x} * {x}",
                            "{x} + 1e-33",
                            "{x} + 1e-34",
                            "{x} * 3",
                            "{x} / 3",
                            "{x} - 1",
                        ]);
                        let text = e.replace("{x}", &xt);
                        p.ops
                            .push(json!({"op": "derive", "v": 1, "as": "d1", "expr": text}));
                        p.schema.columns.push(("d1".into(), Tag::Dec));
                    }
                    3 => {
                        let by = vec![x.clone()];
                        self.op_group(&mut p, &by, None);
                    }
                    _ => {
                        let names = p.schema.names();
                        let by: Vec<String> =
                            names.into_iter().filter(|n| *n != x).take(1).collect();
                        self.op_group(&mut p, &by, Some(x.as_str()));
                    }
                }
                self.finish(&t, &p, false)
            }
            "missing-group" => {
                let wish = TableWish {
                    types: vec![SpecType::Str, SpecType::Float64, SpecType::Int],
                    nan_inf: true,
                    missing_rate: 35,
                    rows: (3, 12),
                    ..plain
                };
                let (t, mut p) = self.start(&wish);
                let mut by = Gen::special_columns(&t);
                if by.is_empty() {
                    by.push(t.first_of(SpecType::Str));
                }
                let k = self.rng.range(1, 2) as usize;
                by = self.rng.sample(&by, k);
                self.op_group(&mut p, &by, None);
                if self.rng.chance(30) {
                    self.op_rank(&mut p, None);
                }
                self.finish(&t, &p, true)
            }
            "missing-key-tie" => {
                let wish = TableWish {
                    types: vec![SpecType::Str, SpecType::Int, SpecType::Float64],
                    nan_inf: true,
                    missing_rate: 35,
                    rows: (4, 12),
                    ..plain
                };
                let (t, mut p) = self.start(&wish);
                let mut by = Gen::special_columns(&t);
                if by.is_empty() {
                    by.push(t.first_of(SpecType::Str));
                }
                let k = self.rng.range(1, 2) as usize;
                by = self.rng.sample(&by, k);
                // Aggregates that tie: counts over a constant-ish column.
                self.op_group(&mut p, &by, None);
                match self.rng.below(3) {
                    0 => {
                        let agg = *self.rng.pick(&["member_count", "a1"]);
                        let dir = if self.rng.chance(50) { "asc" } else { "desc" };
                        let missing = if self.rng.chance(50) { "first" } else { "last" };
                        p.ops.push(json!({"op": "sort", "v": 1, "by": [{"column": agg, "dir": dir}], "missing": missing}));
                        p.schema.ordered = true;
                        if self.rng.chance(60) {
                            self.op_take(&mut p);
                        }
                    }
                    1 => {
                        let by = (*self.rng.pick(&["member_count", "a1"])).to_owned();
                        let missing = *self.rng.pick(&["first", "last", "exclude"]);
                        p.ops.push(json!({"op": "rank", "v": 1, "by": by, "dir": "desc", "ties": "ordinal", "missing": missing, "scope": "all"}));
                        p.schema.columns.push(("rank".into(), Tag::Int));
                        p.schema.ordered = true;
                    }
                    _ => {
                        self.op_rank(&mut p, None);
                        self.op_take(&mut p);
                    }
                }
                let force_missing = self.rng.chance(50);
                self.finish(&t, &p, force_missing)
            }
            "overflow" => {
                let wish = TableWish {
                    types: vec![SpecType::Int, SpecType::Dec],
                    missing_rate: 10,
                    rows: (1, 4),
                    ..plain
                };
                let (t, mut p) = self.start(&wish);
                let int_col = t.first_of(SpecType::Int);
                let col_ref = col_text(&mut self.rng, &int_col);
                let shape = *self.rng.pick(&[
                    "{x} * 9.9e999999 * 10",
                    "({x} + 1) * 1e999999 * 100",
                    "{x} * 1E999999 * {x} * 1e999999",
                    "abs({x}) * 9e999999 * -100",
                    "coalesce({x}, 1) * 5e999999 * 2",
                ]);
                let text = shape.replace("{x}", &col_ref);
                p.ops
                    .push(json!({"op": "derive", "v": 1, "as": "big", "expr": text}));
                p.schema.columns.push(("big".into(), Tag::Dec));
                let count_all = json!({"assert": 1, "kind": "count", "op": "eq", "expected": tv_int(t.rows() as i128)});
                let mut case = self.finish(&t, &p, false);
                case.assertion = count_all;
                case
            }
            _ => {
                // pipeline
                let wish = TableWish {
                    nan_inf: self.rng.chance(50),
                    exponents: self.rng.chance(30),
                    ..plain
                };
                let (t, mut p) = self.start(&wish);
                let n = self.rng.range(2, 5);
                let pw = ExprWish {
                    exponents: wish.exponents,
                    ..ew
                };
                for _ in 0..n {
                    self.op_random(&mut p, &t, pw);
                }
                self.finish(&t, &p, false)
            }
        }
    }
}

// --------------------------------------------------------- validation

/// The first 11.8 or 11.9 refusal of a case, through `tm-formats`.
fn refusal(c: &Case) -> Option<String> {
    let mut schemas = BTreeMap::new();
    for t in &c.tables {
        match ParseSpec::from_json(&t.spec_json()) {
            Ok(s) => {
                schemas.insert(t.name.clone(), TableSchema::from_spec(&s));
            }
            Err(e) => return Some(format!("spec {}: {e}", t.name)),
        }
    }
    tm_checker::rules::validate(Some(&c.steps), &c.assertion, &schemas, None)
        .err()
        .map(|e| e.to_string())
}

fn write_case(dir: &Path, c: &Case, seed: u64, index: u64, bucket: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(dir.join("tables"))?;
    std::fs::create_dir_all(dir.join("specs"))?;
    for t in &c.tables {
        std::fs::write(dir.join("tables").join(t.file_name()), t.file_text())?;
        std::fs::write(
            dir.join("specs").join(format!("{}.json", t.name)),
            pretty(&t.spec_json()),
        )?;
    }
    std::fs::write(dir.join("steps.json"), pretty(&c.steps))?;
    std::fs::write(dir.join("assertion.json"), pretty(&c.assertion))?;
    std::fs::write(
        dir.join("gen.json"),
        pretty(&json!({"seed": seed.to_string(), "index": index, "bucket": bucket})),
    )?;
    Ok(())
}

fn pretty(v: &Value) -> String {
    let mut s = serde_json::to_string_pretty(v).unwrap_or_default();
    s.push('\n');
    s
}

/// Generate case `index` into `out`; returns its bucket and whether it
/// was accepted with a refusal after the redraws.
fn generate(out: &Path, seed: u64, index: u64) -> Result<(&'static str, bool), String> {
    let bucket = BUCKETS[(index % BUCKETS.len() as u64) as usize];
    let error_bucket = ERROR_BUCKETS.contains(&bucket);
    let mut attempt = 0;
    let (case, refused) = loop {
        let mut g = Gen {
            rng: Rng::for_case(seed, index, attempt),
            bucket,
            tables: Vec::new(),
        };
        let c = g.case();
        let r = refusal(&c);
        if error_bucket || r.is_none() || attempt >= REDRAWS {
            break (c, r.is_some());
        }
        attempt += 1;
    };
    let dir = out.join(format!("c{index:05}-{bucket}"));
    write_case(&dir, &case, seed, index, bucket).map_err(|e| format!("{}: {e}", dir.display()))?;
    Ok((bucket, refused))
}

fn usage() -> ExitCode {
    eprintln!("usage: gencases --out DIR --n N [--seed SEED] [--start I]");
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let mut out: Option<PathBuf> = None;
    let mut n: Option<u64> = None;
    let mut seed: u64 = 1;
    let mut start: u64 = 0;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let Some(v) = args.next() else {
            return usage();
        };
        match a.as_str() {
            "--out" => out = Some(PathBuf::from(v)),
            "--n" => n = v.parse().ok(),
            "--seed" => match v.parse() {
                Ok(s) => seed = s,
                Err(_) => return usage(),
            },
            "--start" => match v.parse() {
                Ok(s) => start = s,
                Err(_) => return usage(),
            },
            _ => return usage(),
        }
    }
    let (Some(out), Some(n)) = (out, n) else {
        return usage();
    };
    if let Err(e) = std::fs::create_dir_all(&out) {
        eprintln!("{}: {e}", out.display());
        return ExitCode::FAILURE;
    }
    let mut per_bucket: BTreeMap<&str, (u64, u64)> = BTreeMap::new();
    for index in start..start + n {
        match generate(&out, seed, index) {
            Ok((bucket, refused)) => {
                let e = per_bucket.entry(bucket).or_insert((0, 0));
                e.0 += 1;
                if refused {
                    e.1 += 1;
                }
            }
            Err(e) => {
                eprintln!("{e}");
                return ExitCode::FAILURE;
            }
        }
    }
    eprintln!(
        "gencases: seed {seed}, cases {start}..{} in {}",
        start + n,
        out.display()
    );
    for b in BUCKETS {
        if let Some((count, refused)) = per_bucket.get(b) {
            eprintln!(
                "  {b:<16} {count:>6} cases, {refused:>4} accepted with a refusal after {REDRAWS} redraws"
            );
        }
    }
    ExitCode::SUCCESS
}
