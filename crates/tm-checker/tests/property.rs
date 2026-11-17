//! Item 60: property and metamorphic tests, 1,000 cases each (Verification
//! Plan, Property and metamorphic). Tables are generated with a unique
//! `str` key, a `dec` column with missing values and ties, an `int`
//! column, and a small-domain `str` column; steps are drawn from every op.
//!
//! 1. Permuting the input rows changes nothing.
//! 2. A filter that does not reference `rank` commutes with a sort.
//! 3. `take n` is a prefix of `take n + 1`.
//! 4. `rank` with `ties: ordinal` numbers each scope's rows 1 to its size.
//! 5. Adding a column no step or assertion names changes nothing.

use std::collections::BTreeMap;
use std::sync::Arc;

use arrow::array::ArrayRef;
use arrow::datatypes::{Field, Schema};
use arrow::record_batch::RecordBatch;
use bigdecimal::BigDecimal;
use bigdecimal::num_bigint::BigInt;
use proptest::prelude::*;
use serde_json::{Value, json};

use tm_checker::{InputTable, Outcome, Request, RunResult};
use tm_duckdb::EngineSettings;
use tm_formats::parse_spec::ParseSpec;
use tm_formats::typed_value::{SpecType, TypedValue, arrow_type, to_arrow};

/// One generated row: `x` in tenths (or missing), `y`, `g`.
#[derive(Debug, Clone)]
struct Row {
    x: Option<i64>,
    y: i64,
    g: u8,
}

fn rows() -> impl Strategy<Value = Vec<Row>> {
    prop::collection::vec(
        (prop::option::weighted(0.8, -30i64..30), -5i64..5, 0u8..3).prop_map(|(x, y, g)| Row {
            x,
            y,
            g,
        }),
        0..14,
    )
}

fn spec(extra: bool) -> ParseSpec {
    let mut cols = vec![
        json!({"name":"id","type":"str","missing":[]}),
        json!({"name":"x","type":"dec","missing":["NA"],"scale":1}),
        json!({"name":"y","type":"int","missing":["NA"]}),
        json!({"name":"g","type":"str","missing":["NA"]}),
    ];
    if extra {
        cols.push(json!({"name":"unrelated","type":"int","missing":["NA"]}));
    }
    ParseSpec::from_json(&json!({
        "spec":1,"format":"csv","delimiter":",","header":true,
        "columns": cols, "key":["id"],"nan":"missing","inf":"missing","encoding":"utf-8"
    }))
    .unwrap_or_else(|e| panic!("spec: {e}"))
}

/// The batch for `rows` in the order `order` (indexes into `rows`); row
/// `i` has key `r<i>` whatever its position.
fn batch(rows: &[Row], order: &[usize], extra: bool) -> RecordBatch {
    let col = |f: &dyn Fn(usize, &Row) -> TypedValue| -> Vec<TypedValue> {
        order.iter().map(|i| f(*i, &rows[*i])).collect()
    };
    let mut fields = vec![
        Field::new("id", arrow_type(SpecType::Str, 0), true),
        Field::new("x", arrow_type(SpecType::Dec, 1), true),
        Field::new("y", arrow_type(SpecType::Int, 0), true),
        Field::new("g", arrow_type(SpecType::Str, 0), true),
    ];
    let mk = |v: Vec<TypedValue>, t: SpecType, s: u8| -> ArrayRef {
        to_arrow(&v, t, s).unwrap_or_else(|e| panic!("arrow: {e}"))
    };
    let mut arrays = vec![
        mk(
            col(&|i, _| TypedValue::str_nfc(&format!("r{i:02}"))),
            SpecType::Str,
            0,
        ),
        mk(
            col(&|_, r| {
                r.x.map_or(TypedValue::Missing, |x| {
                    TypedValue::Dec(BigDecimal::new(BigInt::from(x), 1))
                })
            }),
            SpecType::Dec,
            1,
        ),
        mk(col(&|_, r| TypedValue::Int(r.y.into())), SpecType::Int, 0),
        mk(
            col(&|_, r| TypedValue::str_nfc(["a", "b", "c"][usize::from(r.g)])),
            SpecType::Str,
            0,
        ),
    ];
    if extra {
        fields.push(Field::new("unrelated", arrow_type(SpecType::Int, 0), true));
        arrays.push(mk(
            col(&|i, _| TypedValue::Int((i * 7).into())),
            SpecType::Int,
            0,
        ));
    }
    RecordBatch::try_new(Arc::new(Schema::new(fields)), arrays).unwrap_or_else(|e| panic!("{e}"))
}

fn run(batch: RecordBatch, extra: bool, ops: &[Value], assertion: &Value) -> RunResult {
    let mut tables = BTreeMap::new();
    tables.insert(
        "t".to_owned(),
        InputTable {
            handle: "F1@v1".to_owned(),
            spec: spec(extra),
            batch,
        },
    );
    let steps = json!({"steps":1,"tables":{"t":"F1@v1"},"from":"t","ops":ops});
    let settings = EngineSettings::default();
    tm_checker::run(&Request {
        tables: &tables,
        steps: Some(&steps),
        assertion,
        citations: &[],
        settings: &settings,
        limit: usize::MAX,
    })
}

fn count_any() -> Value {
    json!({"assert":1,"kind":"count","op":"ge","expected":{"t":"int","v":"0"}})
}

fn filter() -> impl Strategy<Value = Value> {
    prop_oneof![
        (-30i64..30).prop_map(|v| {
            let sign = if v < 0 { "-" } else { "" };
            json!({"op":"filter","v":1,"where":format!("x > {sign}{}.{}", v.abs() / 10, v.abs() % 10)})
        }),
        (-5i64..5).prop_map(|v| json!({"op":"filter","v":1,"where":format!("y <= {}", v.abs())})),
        prop::sample::select(vec!["a", "b", "c"])
            .prop_map(|g| json!({"op":"filter","v":1,"where":format!("g = \"{g}\" or x = missing")})),
    ]
}

fn sort() -> impl Strategy<Value = Value> {
    (
        prop::sample::select(vec!["x", "y", "g"]),
        prop::bool::ANY,
        prop::sample::select(vec!["x", "y", "g"]),
        prop::bool::ANY,
        prop::bool::ANY,
    )
        .prop_map(|(c1, d1, c2, d2, first)| {
            let dir = |d: bool| if d { "asc" } else { "desc" };
            json!({"op":"sort","v":1,"by":[{"column":c1,"dir":dir(d1)},{"column":c2,"dir":dir(d2)}],
                   "missing": if first {"first"} else {"last"}})
        })
}

fn rank(ties: Option<&'static str>) -> impl Strategy<Value = Value> {
    let ties = match ties {
        Some(t) => Just(t).boxed(),
        None => prop::sample::select(vec!["min", "max", "dense", "ordinal"]).boxed(),
    };
    (
        prop::sample::select(vec!["x", "y"]),
        prop::bool::ANY,
        ties,
        prop::sample::select(vec!["first", "last", "exclude"]),
        prop::bool::ANY,
    )
        .prop_map(|(by, asc, ties, missing, grouped)| {
            let mut op = json!({"op":"rank","v":1,"by":by,"dir":if asc {"asc"} else {"desc"},
                                "ties":ties,"missing":missing,"scope":"all"});
            if grouped {
                op["scope"] = json!("group");
                op["group_by"] = json!(["g"]);
            }
            op
        })
}

/// A pipeline: optional derive, filter, sort or rank, optional take, and
/// sometimes a group.
fn pipeline() -> impl Strategy<Value = Vec<Value>> {
    (
        prop::bool::ANY,
        prop::option::of(filter()),
        prop_oneof![sort(), rank(None)],
        prop::option::of(0u64..6),
        prop::option::of(prop::sample::select(vec![
            "sum", "mean", "min", "max", "median", "count",
        ])),
    )
        .prop_map(|(derive, f, order, take, group)| {
            let mut ops = Vec::new();
            if derive {
                ops.push(json!({"op":"derive","v":1,"as":"z","expr":"x * y - x / 3"}));
            }
            ops.extend(f);
            ops.push(order);
            if let Some(n) = take {
                ops.push(json!({"op":"take","v":1,"n":n}));
            }
            if let Some(func) = group {
                let column = if derive { "z" } else { "x" };
                ops.push(json!({"op":"group","v":1,"by":["g"],
                    "aggregates":[{"as":"agg","fn":func,"column":column}]}));
            }
            ops
        })
}

fn same(a: &RunResult, b: &RunResult) -> Result<(), TestCaseError> {
    prop_assert_eq!(a.verdict.outcome, b.verdict.outcome);
    prop_assert_eq!(&a.verdict.error, &b.verdict.error);
    prop_assert_eq!(&a.verdict.steps, &b.verdict.steps);
    prop_assert_eq!(&a.verdict.final_rows_hash, &b.verdict.final_rows_hash);
    prop_assert_eq!(&a.verdict.key_map_hash, &b.verdict.key_map_hash);
    Ok(())
}

fn ok(r: &RunResult) -> Result<(), TestCaseError> {
    prop_assert!(
        r.verdict.outcome != Outcome::Error,
        "unexpected error {:?}",
        r.verdict.error
    );
    Ok(())
}

fn config() -> ProptestConfig {
    ProptestConfig {
        cases: 1000,
        failure_persistence: None,
        ..ProptestConfig::default()
    }
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn row_permutation_invariance(
        (rows, order) in rows().prop_flat_map(|r| {
            let n = r.len();
            (Just(r), Just((0..n).collect::<Vec<_>>()).prop_shuffle())
        }),
        ops in pipeline(),
    ) {
        let natural: Vec<usize> = (0..rows.len()).collect();
        let a = run(batch(&rows, &natural, false), false, &ops, &count_any());
        let b = run(batch(&rows, &order, false), false, &ops, &count_any());
        ok(&a)?;
        same(&a, &b)?;
    }

    #[test]
    fn filter_commutes_with_sort(rows in rows(), f in filter(), s in sort()) {
        let natural: Vec<usize> = (0..rows.len()).collect();
        let a = run(batch(&rows, &natural, false), false, &[f.clone(), s.clone()], &count_any());
        let b = run(batch(&rows, &natural, false), false, &[s, f], &count_any());
        ok(&a)?;
        prop_assert_eq!(&a.verdict.final_rows_hash, &b.verdict.final_rows_hash);
        prop_assert_eq!(&a.verdict.key_map_hash, &b.verdict.key_map_hash);
    }

    #[test]
    fn take_n_is_a_prefix_of_take_n_plus_1(
        rows in rows(),
        order in prop_oneof![sort(), rank(None)],
        n in 0u64..15,
    ) {
        let natural: Vec<usize> = (0..rows.len()).collect();
        let a = run(batch(&rows, &natural, false), false,
            &[order.clone(), json!({"op":"take","v":1,"n":n})], &count_any());
        let b = run(batch(&rows, &natural, false), false,
            &[order, json!({"op":"take","v":1,"n":n + 1})], &count_any());
        ok(&a)?;
        ok(&b)?;
        let (ra, rb) = (a.final_rows.unwrap_or_default(), b.final_rows.unwrap_or_default());
        prop_assert!(ra.len() <= rb.len() && rb.len() <= ra.len() + 1);
        prop_assert_eq!(&ra[..], &rb[..ra.len()]);
    }

    #[test]
    fn ordinal_rank_is_a_bijection(rows in rows(), op in rank(Some("ordinal"))) {
        let natural: Vec<usize> = (0..rows.len()).collect();
        let r = run(batch(&rows, &natural, false), false, &[op.clone()], &count_any());
        ok(&r)?;
        let cols = r.final_columns.clone().unwrap_or_default();
        let gi = cols.iter().position(|c| c == "g");
        let ri = cols.iter().position(|c| c == "rank");
        let mut scopes: BTreeMap<String, Vec<u64>> = BTreeMap::new();
        for row in r.final_rows.unwrap_or_default() {
            let scope = if op["scope"] == "group" {
                gi.map(|i| row[i].to_json().to_string()).unwrap_or_default()
            } else {
                String::new()
            };
            let rank = match ri.map(|i| &row[i]) {
                Some(TypedValue::Int(n)) => n.to_string().parse::<u64>().unwrap_or(0),
                _ => 0,
            };
            scopes.entry(scope).or_default().push(rank);
        }
        for (_, mut ranks) in scopes {
            ranks.sort_unstable();
            let want: Vec<u64> = (1..=ranks.len() as u64).collect();
            prop_assert_eq!(ranks, want);
        }
    }

    #[test]
    fn an_unrelated_column_changes_nothing(rows in rows(), ops in pipeline()) {
        let natural: Vec<usize> = (0..rows.len()).collect();
        let a = run(batch(&rows, &natural, false), false, &ops, &count_any());
        let b = run(batch(&rows, &natural, true), true, &ops, &count_any());
        ok(&a)?;
        prop_assert_eq!(a.verdict.outcome, b.verdict.outcome);
        prop_assert_eq!(&a.verdict.steps, &b.verdict.steps);
        prop_assert_eq!(&a.verdict.key_map_hash, &b.verdict.key_map_hash);
        let drop_col = |r: &RunResult| -> Vec<Vec<TypedValue>> {
            let cols = r.final_columns.clone().unwrap_or_default();
            let skip = cols.iter().position(|c| c == "unrelated");
            r.final_rows.clone().unwrap_or_default().into_iter().map(|row| {
                row.into_iter().enumerate().filter(|(i, _)| Some(*i) != skip).map(|(_, v)| v).collect()
            }).collect()
        };
        prop_assert_eq!(drop_col(&a), drop_col(&b));
    }
}
