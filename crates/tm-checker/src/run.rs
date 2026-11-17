//! Step-prefix execution (design item 55): validate, read the cited
//! tables, put the start table in key order (step 0), run each op in turn
//! (`filter` and `derive` in Rust, the relational ops in `DuckDB`), hash
//! the final table and its key map (11.18.2), and evaluate the assertion
//! (11.18.3).
//!
//! The result carries the stored [`Verdict`] and, beside it, what the
//! claim page and `claims.evidence` show (11.20, section 10): every step's
//! table from step 0 on, each capped at the caller's limit with its true
//! row count; the rows the assertion names, emphasized, with their
//! positions; every such row of the final step beyond the cap; and the
//! emphasized columns. None of it enters the `claim.checked` body.

use std::collections::BTreeMap;

use arrow::record_batch::RecordBatch;
use serde_json::{Value, json};

use tm_duckdb::EngineSettings;
use tm_formats::Code;
use tm_formats::assertion::{Assertion, Expected, Observed};
use tm_formats::citation::Citation;
use tm_formats::errors::{Error, ptr};
use tm_formats::parse_spec::ParseSpec;
use tm_formats::steps::{AggFn, Op, TableSchema};
use tm_formats::typed_value::{Tag, TypedValue, dec_from_f64_round15};

use crate::describe::describe;
use crate::engine::{self, CheckEngine};
use crate::eval::{self, bind};
use crate::rules::{self, RULES_VERSION};
use crate::table::{Table, encode};
use crate::translate::{self, TRANSLATOR_VERSION, input_table, step_table};
use crate::verdict::{AssertionResult, CheckError, Outcome, StepSummary, Verdict, Versions};

/// One cited table as Files hands it over (4.4): its version handle, its
/// parse spec, and its rows as Arrow (item 47's mapping).
#[derive(Debug, Clone)]
pub struct InputTable {
    /// The version handle, for example `F7@v2`.
    pub handle: String,
    /// The parse spec the table was read with.
    pub spec: ParseSpec,
    /// The parsed rows.
    pub batch: RecordBatch,
}

/// What a check needs (4.4).
#[derive(Debug, Clone, Copy)]
pub struct Request<'a> {
    /// Cited tables by the name the steps give them (`tables` in 11.8).
    /// With `steps` null, the table a `cells` citation names, found by its
    /// handle.
    pub tables: &'a BTreeMap<String, InputTable>,
    /// The 11.8 steps, or `None` for an assertion over a `cells` citation.
    pub steps: Option<&'a Value>,
    /// The 11.9 assertion.
    pub assertion: &'a Value,
    /// The claim's citations (11.23); `cells` citations give emphasized
    /// columns and, with `steps` null, the final rows.
    pub citations: &'a [Citation],
    /// Engine settings (11.19).
    pub settings: &'a EngineSettings,
    /// Rows kept per step table (`view.rows_per_page` or the caller's).
    pub limit: usize,
}

/// One step's table, capped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepTable {
    /// The op name; `None` for step 0, the cited input table.
    pub op: Option<&'static str>,
    /// The 11.18.4 sentence; `None` for step 0.
    pub description: Option<String>,
    /// Column names and types.
    pub columns: Vec<(String, Tag)>,
    /// Key columns.
    pub key: Vec<String>,
    /// The first `limit` rows.
    pub rows: Vec<Vec<TypedValue>>,
    /// The true row count.
    pub row_count: u64,
}

/// Which row of the assertion a named row is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// `row` of `value`, `rank`, or `exists`.
    Row,
    /// `left` of `compare_rows`.
    Left,
    /// `right` of `compare_rows`.
    Right,
}

/// A row the assertion names, emphasized on the final step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamedRow {
    /// Which of the assertion's rows.
    pub role: Role,
    /// The key values the assertion gives.
    pub key: Vec<TypedValue>,
    /// The 1-based position in the final table, `None` when absent.
    pub position: Option<u64>,
}

/// A row the assertion names that lies beyond the final step's cap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowBeyondCap {
    /// Which of the assertion's rows.
    pub role: Role,
    /// The 1-based position ("row 4,812 of 20,000").
    pub position: u64,
    /// The final table's true row count.
    pub of: u64,
    /// The row.
    pub row: Vec<TypedValue>,
}

/// A check's result (item 55).
#[derive(Debug, Clone, PartialEq)]
pub struct RunResult {
    /// The stored verdict.
    pub verdict: Verdict,
    /// Step 0, then one table per op that ran; on a runtime error, the
    /// steps before it. Empty when validation refused the check.
    pub steps: Vec<StepTable>,
    /// The rows the assertion names (`row`, `left`, `right`); empty for
    /// `count`.
    pub emphasized_rows: Vec<NamedRow>,
    /// Every row the assertion names that is present at a position beyond
    /// the cap of the final step.
    pub beyond_cap: Vec<RowBeyondCap>,
    /// The column or columns the assertion names, then the columns any
    /// `cells` citation lists, each once, kept when the final table has
    /// it.
    pub emphasized_columns: Vec<String>,
    /// The final table's column names, uncapped result only (`None` on
    /// error).
    pub final_columns: Option<Vec<String>>,
    /// The final table's rows when they number at most `limit`, for
    /// callers that need them all (`None` on error or when capped).
    pub final_rows: Option<Vec<Vec<TypedValue>>>,
}

fn cap(
    t: &Table,
    op: Option<&'static str>,
    description: Option<String>,
    limit: usize,
) -> StepTable {
    StepTable {
        op,
        description,
        columns: t.columns.clone(),
        key: t.key.clone(),
        rows: t.rows.iter().take(limit).cloned().collect(),
        row_count: t.rows.len() as u64,
    }
}

fn op_name(op: &Op) -> &'static str {
    match op {
        Op::Filter { .. } => "filter",
        Op::Select { .. } => "select",
        Op::Sort { .. } => "sort",
        Op::Take { .. } => "take",
        Op::Rank { .. } => "rank",
        Op::Derive { .. } => "derive",
        Op::Group { .. } => "group",
        Op::Join { .. } => "join",
    }
}

struct Ctx<'a> {
    req: &'a Request<'a>,
    engine: Option<CheckEngine>,
    sql: Vec<String>,
    tables: Vec<StepTable>,
    summaries: Vec<StepSummary>,
    registered: Vec<String>,
}

/// Run one check. Every failure is a verdict `error` with its code; this
/// function does not fail.
#[must_use]
pub fn run(req: &Request<'_>) -> RunResult {
    let mut parse_specs = BTreeMap::new();
    for t in req.tables.values() {
        parse_specs.insert(t.handle.clone(), t.spec.fingerprint().as_str().to_owned());
    }
    let mut versions = Versions {
        parse_specs,
        translator: TRANSLATOR_VERSION.to_owned(),
        duckdb: "unknown".to_owned(),
        extensions: Vec::new(),
        rules: RULES_VERSION.to_owned(),
        build: tm_duckdb::build_flavour().to_owned(),
    };
    let mut ctx = Ctx {
        req,
        engine: None,
        sql: Vec::new(),
        tables: Vec::new(),
        summaries: Vec::new(),
        registered: Vec::new(),
    };
    match CheckEngine::open(req.settings).and_then(|e| e.facts().map(|f| (e, f))) {
        Ok((e, f)) => {
            versions.duckdb = f.version;
            versions.extensions = f.extensions;
            f.build.clone_into(&mut versions.build);
            ctx.engine = Some(e);
        }
        Err(e) => return failed(ctx, versions, e, None),
    }
    let schemas: BTreeMap<String, TableSchema> = req
        .tables
        .iter()
        .map(|(n, t)| (n.clone(), TableSchema::from_spec(&t.spec)))
        .collect();
    let cited = if req.steps.is_none() {
        cells_table(req).map(|(_, t)| TableSchema::from_spec(&t.spec))
    } else {
        None
    };
    let v = match rules::validate(req.steps, req.assertion, &schemas, cited.as_ref()) {
        Ok(v) => v,
        Err(e) => return failed(ctx, versions, e, None),
    };
    let final_table = match &v.steps {
        Some(steps) => execute(&mut ctx, &steps.from, &steps.ops),
        None => cells_rows(&mut ctx),
    };
    let final_table = match final_table {
        Ok(t) => t,
        Err(e) => return failed(ctx, versions, e, None),
    };
    finish(ctx, versions, &v.assertion, &final_table)
}

fn failed(
    ctx: Ctx<'_>,
    versions: Versions,
    e: Error,
    assertion: Option<AssertionResult>,
) -> RunResult {
    RunResult {
        verdict: Verdict {
            outcome: Outcome::Error,
            assertion,
            steps: Vec::new(),
            final_rows_hash: None,
            key_map_hash: None,
            key_map: Vec::new(),
            sql: join_sql(&ctx.sql),
            versions,
            error: Some(CheckError::from(e)),
        },
        steps: ctx.tables,
        emphasized_rows: Vec::new(),
        beyond_cap: Vec::new(),
        emphasized_columns: Vec::new(),
        final_columns: None,
        final_rows: None,
    }
}

fn join_sql(parts: &[String]) -> String {
    parts.join("\n\n")
}

/// The input table a `cells` citation names, by handle.
fn cells_table<'a>(req: &'a Request<'_>) -> Option<(&'a String, &'a InputTable)> {
    let handle = req.citations.iter().find_map(|c| match c {
        Citation::Cells { version, .. } => Some(version.to_string()),
        _ => None,
    })?;
    req.tables.iter().find(|(_, t)| t.handle == handle)
}

impl Ctx<'_> {
    fn engine(&mut self) -> Result<&mut CheckEngine, Error> {
        self.engine
            .as_mut()
            .ok_or_else(|| Error::new(Code::E5001, "the engine is not open"))
    }

    fn register(
        &mut self,
        name: &str,
        t: &Table,
        extras: &crate::table::Extras,
    ) -> Result<(), Error> {
        let batch = encode(t, extras)?;
        self.engine()?.register(name, batch)?;
        self.registered.push(name.to_owned());
        Ok(())
    }

    fn query(&mut self, sql: &str) -> Result<RecordBatch, Error> {
        self.sql.push(format!("{sql};"));
        self.engine()?.query(sql)
    }
}

fn load(req: &Request<'_>, name: &str) -> Result<Table, Error> {
    let t = req.tables.get(name).ok_or_else(|| {
        Error::at(
            Code::E6002,
            ptr("/tables", name),
            format!("no table {name:?}"),
        )
    })?;
    Table::from_arrow(&t.batch, &t.spec)
}

/// Step 0: the start table in key order, in `DuckDB`.
fn step0(ctx: &mut Ctx<'_>, name: &str) -> Result<Table, Error> {
    let t = load(ctx.req, name)?;
    let st = translate::initial(name, &t);
    ctx.register(&input_table(name), &t, &st.extras())?;
    let b = ctx.query(&st.sql)?;
    let order: Vec<usize> = engine::rows(&b, 0)?.into_iter().flatten().collect();
    Ok(t.reorder(&order))
}

fn execute(ctx: &mut Ctx<'_>, from: &str, ops: &[Op]) -> Result<Table, Error> {
    let mut cur = step0(ctx, from)?;
    ctx.tables.push(cap(&cur, None, None, ctx.req.limit));
    for (i, op) in ops.iter().enumerate() {
        let at = ptr("/ops", i);
        let m = cur.rows.len() as u64;
        let input_key = cur.key.clone();
        let next = apply(ctx, i, op, &at, &cur)?;
        let n = next.rows.len() as u64;
        let description = describe(op, &input_key, m, n);
        ctx.summaries.push(StepSummary {
            op: op_name(op),
            rows_out: n,
            description: description.clone(),
        });
        ctx.tables.push(cap(
            &next,
            Some(op_name(op)),
            Some(description),
            ctx.req.limit,
        ));
        cur = next;
    }
    Ok(cur)
}

#[allow(clippy::too_many_lines, reason = "one arm per op")]
fn apply(ctx: &mut Ctx<'_>, i: usize, op: &Op, at: &str, cur: &Table) -> Result<Table, Error> {
    let input = step_table(i);
    match op {
        Op::Filter { where_ } => {
            let wat = ptr(at, "where");
            let b = bind(where_, cur, &wat)?;
            let mut keep = Vec::new();
            for r in 0..cur.rows.len() {
                match b.eval(cur, r, &wat)? {
                    TypedValue::Bool(true) => keep.push(r),
                    TypedValue::Bool(false)
                    | TypedValue::Missing
                    | TypedValue::Nan
                    | TypedValue::Inf(_) => {}
                    other => {
                        return Err(Error::at(
                            Code::E6004,
                            wat,
                            format!("`where` gave {:?}, not a boolean", other.tag()),
                        ));
                    }
                }
            }
            ctx.sql.push(format!(
                "-- ops/{i} filter: evaluated in Rust over {input} (11.18.2 Where); its rows are {}",
                step_table(i + 1)
            ));
            Ok(cur.reorder(&keep))
        }
        Op::Derive { as_, expr } => {
            let eat = ptr(at, "expr");
            let tag = expr
                .type_of(&|c| cur.tag(c), &eat)?
                .ok_or_else(|| Error::at(Code::E6004, &eat, "a derived column needs a type"))?;
            let b = bind(expr, cur, &eat)?;
            let mut out = cur.clone();
            for r in 0..cur.rows.len() {
                let v = match (b.eval(cur, r, &eat)?, tag) {
                    (TypedValue::Int(n), Tag::Dec) => {
                        TypedValue::Dec(bigdecimal::BigDecimal::new(n, 0))
                    }
                    (v, _) => v,
                };
                out.rows[r].push(v);
            }
            out.columns.push((as_.clone(), tag));
            ctx.sql.push(format!(
                "-- ops/{i} derive: column {as_:?} computed in Rust over {input} (11.18.2 Where); {} is {input} with it as column c{}",
                step_table(i + 1),
                cur.columns.len()
            ));
            Ok(out)
        }
        Op::Select { columns } => {
            let st = translate::statement(op, &input, cur, None)
                .ok_or_else(|| Error::new(Code::E5001, "no statement for select"))?;
            ctx.register(&input, cur, &st.extras())?;
            let b = ctx.query(&st.sql)?;
            let order: Vec<usize> = engine::rows(&b, 0)?.into_iter().flatten().collect();
            let keep: Vec<usize> = (0..cur.columns.len())
                .filter(|j| {
                    let n = &cur.columns[*j].0;
                    columns.contains(n) || cur.key.contains(n)
                })
                .collect();
            Ok(Table {
                columns: keep.iter().map(|j| cur.columns[*j].clone()).collect(),
                key: cur.key.clone(),
                rows: order
                    .iter()
                    .map(|r| keep.iter().map(|j| cur.rows[*r][*j].clone()).collect())
                    .collect(),
            })
        }
        Op::Sort { .. } | Op::Take { .. } => {
            let st = translate::statement(op, &input, cur, None)
                .ok_or_else(|| Error::new(Code::E5001, "no statement"))?;
            ctx.register(&input, cur, &st.extras())?;
            let b = ctx.query(&st.sql)?;
            let order: Vec<usize> = engine::rows(&b, 0)?.into_iter().flatten().collect();
            Ok(cur.reorder(&order))
        }
        Op::Rank { .. } => {
            let st = translate::statement(op, &input, cur, None)
                .ok_or_else(|| Error::new(Code::E5001, "no statement"))?;
            ctx.register(&input, cur, &st.extras())?;
            let b = ctx.query(&st.sql)?;
            let order = engine::rows(&b, 0)?;
            let ranks = engine::rows(&b, 1)?;
            let mut out = Table {
                columns: cur.columns.clone(),
                key: cur.key.clone(),
                rows: Vec::with_capacity(order.len()),
            };
            out.columns.push(("rank".to_owned(), Tag::Int));
            for (r, k) in order.iter().zip(&ranks) {
                let (Some(r), Some(k)) = (r, k) else {
                    return Err(Error::new(Code::E5001, "rank returned NULL"));
                };
                let mut row = cur.rows[*r].clone();
                row.push(TypedValue::Int((*k).into()));
                out.rows.push(row);
            }
            Ok(out)
        }
        Op::Group { by, aggregates } => group(ctx, &input, op, at, cur, by, aggregates),
        Op::Join {
            right,
            on: _,
            left_kind: _,
        } => {
            let rt = load(ctx.req, right)?;
            let rname = input_table(right);
            let st = translate::statement(op, &input, cur, Some((&rname, &rt)))
                .ok_or_else(|| Error::new(Code::E5001, "no statement for join"))?;
            ctx.register(&input, cur, &st.extras())?;
            if !ctx.registered.contains(&rname) {
                ctx.register(&rname, &rt, &crate::table::Extras::default())?;
            }
            let b = ctx.query(&st.sql)?;
            let lefts = engine::rows(&b, 0)?;
            let rights = engine::rows(&b, 1)?;
            let mut out = Table {
                columns: cur.columns.clone(),
                key: cur.key.clone(),
                rows: Vec::with_capacity(lefts.len()),
            };
            for (c, t) in &rt.columns {
                out.columns.push((format!("{right}.{c}"), *t));
            }
            for (l, r) in lefts.iter().zip(&rights) {
                let Some(l) = l else {
                    return Err(Error::new(Code::E5001, "join returned no left row"));
                };
                let mut row = cur.rows[*l].clone();
                match r {
                    Some(r) => row.extend(rt.rows[*r].iter().cloned()),
                    None => row.extend(std::iter::repeat_n(TypedValue::Missing, rt.columns.len())),
                }
                out.rows.push(row);
            }
            Ok(out)
        }
    }
}

fn group(
    ctx: &mut Ctx<'_>,
    input: &str,
    op: &Op,
    at: &str,
    cur: &Table,
    by: &[String],
    aggregates: &[tm_formats::steps::Aggregate],
) -> Result<Table, Error> {
    let st = translate::statement(op, input, cur, None)
        .ok_or_else(|| Error::new(Code::E5001, "no statement for group"))?;
    ctx.register(input, cur, &st.extras())?;
    let b = ctx.query(&st.sql)?;
    let reps = engine::rows(&b, 0)?;
    let members = engine::row_lists(&b, 1)?;
    let counts = engine::rows(&b, 2)?;
    let hashes = engine::texts(&b, 3)?;
    let by_idx: Vec<usize> = by.iter().filter_map(|c| cur.index(c)).collect();
    let mut out = Table {
        columns: by_idx.iter().map(|j| cur.columns[*j].clone()).collect(),
        key: by.to_vec(),
        rows: Vec::with_capacity(reps.len()),
    };
    // One column of values per aggregate, one value per group.
    let mut agg_values: Vec<Vec<TypedValue>> = Vec::with_capacity(aggregates.len());
    for (a, agg) in aggregates.iter().enumerate() {
        let aat = ptr(&ptr(at, "aggregates"), a);
        let j = cur
            .index(&agg.column)
            .ok_or_else(|| Error::at(Code::E6005, &aat, "unknown column"))?;
        let tag = cur.columns[j].1;
        let out_tag = match agg.func {
            AggFn::Count => Tag::Int,
            AggFn::Mean | AggFn::Median => Tag::Dec,
            AggFn::Sum | AggFn::Min | AggFn::Max => tag,
        };
        out.columns.push((agg.as_.clone(), out_tag));
        let c = 4 + a;
        let values: Vec<TypedValue> = match agg.func {
            AggFn::Count => engine::rows(&b, c)?
                .into_iter()
                .map(|n| TypedValue::Int(n.unwrap_or(0).into()))
                .collect(),
            AggFn::Sum => members
                .iter()
                .map(|m| eval::sum(m.iter().map(|r| &cur.rows[*r][j]), tag, &aat))
                .collect::<Result<_, _>>()?,
            AggFn::Mean | AggFn::Median => engine::doubles(&b, c)?
                .into_iter()
                .map(|x| {
                    x.and_then(dec_from_f64_round15)
                        .map_or(TypedValue::Missing, TypedValue::Dec)
                })
                .collect(),
            AggFn::Min | AggFn::Max => engine::rows(&b, c)?
                .into_iter()
                .map(|r| r.map_or(TypedValue::Missing, |r| cur.rows[r][j].clone()))
                .collect(),
        };
        agg_values.push(values);
    }
    out.columns.push(("member_count".to_owned(), Tag::Int));
    out.columns.push(("member_keys_hash".to_owned(), Tag::Str));
    for g in 0..reps.len() {
        let rep = reps[g].ok_or_else(|| Error::new(Code::E5001, "a group with no rows"))?;
        let mut row: Vec<TypedValue> = by_idx.iter().map(|j| cur.rows[rep][*j].clone()).collect();
        for values in &agg_values {
            row.push(values.get(g).cloned().unwrap_or(TypedValue::Missing));
        }
        row.push(TypedValue::Int(counts[g].unwrap_or(0).into()));
        row.push(TypedValue::Str(hashes[g].clone().unwrap_or_default()));
        out.rows.push(row);
    }
    Ok(out)
}

/// With `steps` null: the rows the `cells` citations of the cited table
/// name, each once, in key order (11.8). Step 0 is that table.
fn cells_rows(ctx: &mut Ctx<'_>) -> Result<Table, Error> {
    let req = ctx.req;
    let Some((name, it)) = cells_table(req) else {
        return Err(Error::new(
            Code::E5001,
            "an assertion with steps null needs a cells citation of a supplied table",
        ));
    };
    let name = name.clone();
    let mut keys: Vec<Vec<TypedValue>> = Vec::new();
    for c in req.citations {
        if let Citation::Cells {
            version, keys: k, ..
        } = c
            && version.to_string() == it.handle
        {
            keys.extend(k.iter().cloned());
        }
    }
    let t = step0(ctx, &name)?;
    let tuples = t.key_tuples();
    let keep: Vec<usize> = (0..t.rows.len())
        .filter(|r| keys.iter().any(|k| same_key(k, &tuples[*r])))
        .collect();
    let out = t.reorder(&keep);
    ctx.tables.push(cap(&out, None, None, ctx.req.limit));
    Ok(out)
}

fn same_key(a: &[TypedValue], b: &[TypedValue]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(x, y)| x.compare(y) == Some(std::cmp::Ordering::Equal))
}

fn position(tuples: &[Vec<TypedValue>], key: &[TypedValue]) -> Option<usize> {
    tuples.iter().position(|k| same_key(key, k))
}

fn hash_of(v: &Value) -> Result<String, Error> {
    Ok(tm_formats::id::id_of(v)?.as_str().to_owned())
}

#[allow(
    clippy::too_many_lines,
    reason = "locating each assertion kind reads best in one place"
)]
fn finish(ctx: Ctx<'_>, versions: Versions, assertion: &Assertion, t: &Table) -> RunResult {
    let tuples = t.key_tuples();
    let names = t.names();
    let rows_json: Vec<Value> = t
        .rows
        .iter()
        .map(|r| {
            Value::Object(
                names
                    .iter()
                    .zip(r)
                    .map(|(n, v)| (n.clone(), v.to_json()))
                    .collect(),
            )
        })
        .collect();
    let key_json: Vec<Value> = tuples
        .iter()
        .map(|k| Value::Array(k.iter().map(TypedValue::to_json).collect()))
        .collect();
    let hashes = hash_of(&Value::Array(rows_json))
        .and_then(|f| hash_of(&Value::Array(key_json)).map(|k| (f, k)));
    let (final_rows_hash, key_map_hash) = match hashes {
        Ok(h) => h,
        Err(e) => return failed(ctx, versions, e, None),
    };

    // Locate what the assertion names (11.18.3).
    let cell = |key: &[TypedValue], column: &str| -> Option<TypedValue> {
        let r = position(&tuples, key)?;
        Some(t.rows[r][t.index(column)?].clone())
    };
    let mut assertion_rows: Vec<(Role, Vec<TypedValue>)> = Vec::new();
    let mut columns: Vec<String> = Vec::new();
    let (observed, observed_json, expected_json) = match assertion {
        Assertion::Value {
            cell: c, expected, ..
        } => {
            assertion_rows.push((Role::Row, c.row.clone()));
            columns.push(c.column.clone());
            let v = cell(&c.row, &c.column);
            (
                Obs::Cell(v.clone()),
                json!({"cell": v.as_ref().map(TypedValue::to_json)}),
                expected_value(expected),
            )
        }
        Assertion::Rank { row, expected, .. } => {
            assertion_rows.push((Role::Row, row.clone()));
            let p = position(&tuples, row).map(|p| p as u64 + 1);
            (Obs::Position(p), json!({"position": p}), expected.to_json())
        }
        Assertion::Count { expected, .. } => {
            let n = t.rows.len() as u64;
            (Obs::Count(n), json!({"count": n}), expected_value(expected))
        }
        Assertion::CompareRows { left, right, .. } => {
            assertion_rows.push((Role::Left, left.row.clone()));
            assertion_rows.push((Role::Right, right.row.clone()));
            columns.push(left.column.clone());
            columns.push(right.column.clone());
            let (l, r) = (
                cell(&left.row, &left.column),
                cell(&right.row, &right.column),
            );
            (
                Obs::Cells(l.clone(), r.clone()),
                json!({"left": l.as_ref().map(TypedValue::to_json), "right": r.as_ref().map(TypedValue::to_json)}),
                Value::Null,
            )
        }
        Assertion::Exists { row } => {
            assertion_rows.push((Role::Row, row.clone()));
            let p = position(&tuples, row).is_some();
            (Obs::Present(p), json!({"present": p}), Value::Null)
        }
    };
    for c in ctx.req.citations {
        if let Citation::Cells {
            columns: Some(cs), ..
        } = c
        {
            columns.extend(cs.iter().cloned());
        }
    }
    let mut emphasized_columns: Vec<String> = Vec::new();
    for c in columns {
        if t.index(&c).is_some() && !emphasized_columns.contains(&c) {
            emphasized_columns.push(c);
        }
    }
    let total = t.rows.len() as u64;
    let limit = ctx.req.limit;
    let mut emphasized_rows = Vec::new();
    let mut beyond_cap = Vec::new();
    for (role, key) in assertion_rows {
        let p = position(&tuples, &key);
        if let Some(p) = p
            && p >= limit
        {
            beyond_cap.push(RowBeyondCap {
                role,
                position: p as u64 + 1,
                of: total,
                row: t.rows[p].clone(),
            });
        }
        emphasized_rows.push(NamedRow {
            role,
            key,
            position: p.map(|p| p as u64 + 1),
        });
    }

    let outcome = assertion.evaluate(observed.as_observed());
    let mut verdict = Verdict {
        outcome: Outcome::Error,
        assertion: None,
        steps: ctx.summaries.clone(),
        final_rows_hash: Some(final_rows_hash),
        key_map_hash: Some(key_map_hash),
        key_map: tuples,
        sql: join_sql(&ctx.sql),
        versions,
        error: None,
    };
    match outcome {
        Ok(o) => {
            let o = match o {
                tm_formats::assertion::Outcome::Holds => Outcome::Holds,
                tm_formats::assertion::Outcome::Fails => Outcome::Fails,
            };
            verdict.outcome = o;
            verdict.assertion = Some(AssertionResult {
                outcome: o,
                observed: observed_json,
                expected: expected_json,
            });
        }
        Err(e) => {
            let a = AssertionResult {
                outcome: Outcome::Error,
                observed: observed_json,
                expected: expected_json,
            };
            let mut r = failed(ctx, verdict.versions, e, Some(a));
            r.emphasized_rows = emphasized_rows;
            r.beyond_cap = beyond_cap;
            r.emphasized_columns = emphasized_columns;
            return r;
        }
    }
    let all = t.rows.len() <= limit;
    RunResult {
        verdict,
        steps: ctx.tables,
        emphasized_rows,
        beyond_cap,
        emphasized_columns,
        final_columns: Some(names),
        final_rows: all.then(|| t.rows.clone()),
    }
}

fn expected_value(e: &Expected) -> Value {
    match e {
        Expected::One(v) => v.to_json(),
        Expected::Range(lo, hi) => Value::Array(vec![lo.to_json(), hi.to_json()]),
    }
}

/// What was located, owned.
enum Obs {
    Cell(Option<TypedValue>),
    Position(Option<u64>),
    Count(u64),
    Cells(Option<TypedValue>, Option<TypedValue>),
    Present(bool),
}

impl Obs {
    fn as_observed(&self) -> Observed<'_> {
        match self {
            Obs::Cell(v) => Observed::Cell(v.as_ref()),
            Obs::Position(p) => Observed::Position(*p),
            Obs::Count(n) => Observed::Count(*n),
            Obs::Cells(l, r) => Observed::Cells(l.as_ref(), r.as_ref()),
            Obs::Present(p) => Observed::Present(*p),
        }
    }
}

/// Evaluate an assertion against what was located, given in the form
/// [`AssertionResult::observed`] records (an `assertions/eval/` case,
/// item 92): the 11.9 shape checks, then 11.18.3.
///
/// # Errors
/// The 11.9 or 11.18.3 error, or `E5001` for an `observed` that does not
/// match the assertion's kind.
pub fn evaluate_located(assertion: &Value, observed: &Value) -> Result<Outcome, CheckError> {
    let a = Assertion::from_json(assertion)?;
    let bad = || {
        CheckError::from(Error::new(
            Code::E5001,
            "observed does not match the assertion kind",
        ))
    };
    let typed = |v: &Value| -> Result<Option<TypedValue>, CheckError> {
        if v.is_null() {
            Ok(None)
        } else {
            Ok(Some(TypedValue::from_json(v, "/observed")?))
        }
    };
    let obs = match &a {
        Assertion::Value { .. } => Obs::Cell(typed(observed.get("cell").ok_or_else(bad)?)?),
        Assertion::Rank { .. } => Obs::Position(observed.get("position").ok_or_else(bad)?.as_u64()),
        Assertion::Count { .. } => Obs::Count(
            observed
                .get("count")
                .and_then(Value::as_u64)
                .ok_or_else(bad)?,
        ),
        Assertion::CompareRows { .. } => Obs::Cells(
            typed(observed.get("left").ok_or_else(bad)?)?,
            typed(observed.get("right").ok_or_else(bad)?)?,
        ),
        Assertion::Exists { .. } => Obs::Present(
            observed
                .get("present")
                .and_then(Value::as_bool)
                .ok_or_else(bad)?,
        ),
    };
    match a.evaluate(obs.as_observed())? {
        tm_formats::assertion::Outcome::Holds => Ok(Outcome::Holds),
        tm_formats::assertion::Outcome::Fails => Ok(Outcome::Fails),
    }
}
