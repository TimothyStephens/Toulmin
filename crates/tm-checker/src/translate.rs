//! Relational steps to explicit `DuckDB` SQL (design 11.18.2, item 53).
//!
//! Each relational op (`sort`, `take`, `rank`, `group`, `join`, `select`)
//! is one statement over the table registered for its input step,
//! encoded as [`crate::table::encode`] describes: `row` is the input row
//! number and `c<j>` the cell of column `j`. A statement returns row
//! numbers (and ranks, or group results); the Checker assembles the output
//! rows from them. Every ordering is explicit: binary collation on text
//! (11.10), `NULLS FIRST` or `NULLS LAST` from the op's `missing` rule,
//! and the table key appended ascending as the final tie-breaker. A
//! `filter` or `derive` step is evaluated in Rust (11.18.2 Where); the
//! stored SQL names its output as the next statement's input table.

use std::fmt::Write as _;

use tm_formats::steps::{AggFn, Dir, MissingPlace, Op, Ties};
use tm_formats::typed_value::Tag;

use crate::table::{Extras, KEY, ROW, Table, col, dbl};

/// The translator's version, recorded in every verdict's `versions`.
pub const TRANSLATOR_VERSION: &str = "tm-checker-translate-1";

/// The collation clause on every text ordering and grouping (11.10).
/// `DuckDB` refuses `COLLATE BINARY` (`BINARY` is a keyword) and accepts
/// the quoted name (11.10).
pub const COLLATE: &str = "COLLATE \"binary\"";

/// The SQL name of the table registered for step `i` (step 0 is the cited
/// input table in key order).
#[must_use]
pub fn step_table(i: usize) -> String {
    format!("step{i}")
}

/// The SQL name of the registered input table `name`, before key order.
#[must_use]
pub fn input_table(name: &str) -> String {
    format!("input.{name}")
}

/// Double-quote an identifier.
#[must_use]
pub fn q(name: &str) -> String {
    tm_duckdb::quote_ident(name)
}

/// One translated statement: its SQL and what the input table must carry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Statement {
    /// The SQL, a header comment naming the columns, then one `SELECT`.
    pub sql: String,
    /// Extra columns the encoded input needs.
    pub key_json: bool,
    /// Columns that need a double for `mean` or `median`.
    pub doubles: Vec<usize>,
}

impl Statement {
    /// The encoding extras this statement needs.
    #[must_use]
    pub fn extras(&self) -> Extras {
        Extras {
            key_json: self.key_json,
            doubles: self.doubles.clone(),
        }
    }
}

fn tag_name(t: Tag) -> &'static str {
    t.as_str()
}

/// The `-- c<j> = <name> (<type>)` header for a table.
fn header(table_sql: &str, t: &Table) -> String {
    let cols: Vec<String> = t
        .columns
        .iter()
        .enumerate()
        .map(|(j, (n, tag))| format!("{} = {n:?} ({})", col(j), tag_name(*tag)))
        .collect();
    let key: Vec<String> = t.key.iter().map(|k| format!("{k:?}")).collect();
    format!(
        "-- {table_sql}: {}; key {}\n",
        cols.join(", "),
        key.join(", ")
    )
}

/// An ordering term on column `j` of `t`, qualified by `alias` when given.
fn term(t: &Table, j: usize, alias: &str) -> String {
    let c = format!("{alias}{}", q(&col(j)));
    if t.columns[j].1 == Tag::Bool {
        c
    } else {
        format!("{c} {COLLATE}")
    }
}

fn dir(d: Dir) -> &'static str {
    match d {
        Dir::Asc => "ASC",
        Dir::Desc => "DESC",
    }
}

fn nulls(m: MissingPlace) -> &'static str {
    match m {
        MissingPlace::First => "NULLS FIRST",
        MissingPlace::Last | MissingPlace::Exclude => "NULLS LAST",
    }
}

/// The key tie-breaker: every key column ascending. A key value is
/// missing only after a `group` on a column with missing values, and such
/// a group sorts first (11.18.2 `group`), so key terms place `NULL` first.
fn key_terms(t: &Table) -> Vec<String> {
    t.key_indexes()
        .into_iter()
        .map(|j| format!("{} ASC NULLS FIRST", term(t, j, "")))
        .collect()
}

fn idx(t: &Table, name: &str) -> usize {
    t.index(name).unwrap_or(usize::MAX)
}

/// Step 0: the cited input table in key order (11.18.2 Initial order).
#[must_use]
pub fn initial(name: &str, t: &Table) -> Statement {
    let tn = input_table(name);
    let mut sql = header(&tn, t);
    let _ = write!(
        sql,
        "SELECT {} FROM {} ORDER BY {}",
        q(ROW),
        q(&tn),
        key_terms(t).join(", ")
    );
    Statement {
        sql,
        key_json: false,
        doubles: Vec::new(),
    }
}

/// The statement for relational op `op` over step table `input` holding
/// `t`. `right` is the right table of a `join`. `None` for `filter` and
/// `derive`, which are evaluated in Rust.
#[must_use]
#[allow(clippy::too_many_lines, reason = "one arm per relational op")]
pub fn statement(
    op: &Op,
    input: &str,
    t: &Table,
    right: Option<(&str, &Table)>,
) -> Option<Statement> {
    let from = q(input);
    let mut sql = header(input, t);
    let mut key_json = false;
    let mut doubles = Vec::new();
    match op {
        Op::Filter { .. } | Op::Derive { .. } => return None,
        Op::Select { columns } => {
            let mut keep: Vec<usize> = (0..t.columns.len())
                .filter(|j| {
                    let n = &t.columns[*j].0;
                    columns.contains(n) || t.key.contains(n)
                })
                .collect();
            keep.sort_unstable();
            let cols: Vec<String> = std::iter::once(q(ROW))
                .chain(keep.iter().map(|j| q(&col(*j))))
                .collect();
            let _ = write!(
                sql,
                "SELECT {} FROM {from} ORDER BY {}",
                cols.join(", "),
                q(ROW)
            );
        }
        Op::Sort { by, missing } => {
            let mut terms: Vec<String> = by
                .iter()
                .map(|(c, d)| format!("{} {} {}", term(t, idx(t, c), ""), dir(*d), nulls(*missing)))
                .collect();
            terms.extend(key_terms(t));
            let _ = write!(
                sql,
                "SELECT {} FROM {from} ORDER BY {}",
                q(ROW),
                terms.join(", ")
            );
        }
        Op::Take { n } => {
            let _ = write!(
                sql,
                "SELECT {} FROM {from} ORDER BY {} LIMIT {n}",
                q(ROW),
                q(ROW)
            );
        }
        Op::Rank {
            by,
            dir: d,
            ties,
            missing,
            group_by,
        } => {
            let b = idx(t, by);
            let partition = match group_by {
                Some(g) => format!(
                    "PARTITION BY {} ",
                    g.iter()
                        .map(|c| q(&col(idx(t, c))))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                None => String::new(),
            };
            let order = format!("{} {} {}", term(t, b, ""), dir(*d), nulls(*missing));
            let func = match ties {
                Ties::Min => format!("rank() OVER ({partition}ORDER BY {order})"),
                Ties::Dense => format!("dense_rank() OVER ({partition}ORDER BY {order})"),
                Ties::Max => format!(
                    "count(*) OVER ({partition}ORDER BY {order} RANGE BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW)"
                ),
                Ties::Ordinal => format!(
                    "row_number() OVER ({partition}ORDER BY {order}, {})",
                    key_terms(t).join(", ")
                ),
            };
            let filter = if *missing == MissingPlace::Exclude {
                format!(" WHERE {} IS NOT NULL", q(&col(b)))
            } else {
                String::new()
            };
            let mut terms = vec![format!("{} ASC", q("rank"))];
            terms.extend(key_terms(t));
            let _ = write!(
                sql,
                "SELECT {}, {func} AS {} FROM {from}{filter} ORDER BY {}",
                q(ROW),
                q("rank"),
                terms.join(", ")
            );
        }
        Op::Group { by, aggregates } => {
            key_json = true;
            let mut sel = vec![
                format!("first({} ORDER BY {}) AS {}", q(ROW), q(ROW), q("rep")),
                format!("list({} ORDER BY {}) AS {}", q(ROW), q(ROW), q("members")),
                format!("count(*) AS {}", q("member_count")),
                format!(
                    "'sha256:' || sha256('[' || string_agg({}, ',' ORDER BY {} {COLLATE}) || ']') AS {}",
                    q(KEY),
                    q(KEY),
                    q("member_keys_hash")
                ),
            ];
            for (a, agg) in aggregates.iter().enumerate() {
                let j = idx(t, &agg.column);
                let c = q(&col(j));
                let expr = match agg.func {
                    AggFn::Count => "count(*)".to_owned(),
                    // Exact in Rust under the 11.6 context over the
                    // `members` list (docs/decisions.md, 2026-11-17).
                    AggFn::Sum => "NULL".to_owned(),
                    AggFn::Mean => {
                        doubles.push(j);
                        format!("avg({})", q(&dbl(j)))
                    }
                    AggFn::Median => {
                        doubles.push(j);
                        format!("median({})", q(&dbl(j)))
                    }
                    AggFn::Min | AggFn::Max => format!(
                        "first({} ORDER BY {} {}, {}) FILTER (WHERE {c} IS NOT NULL)",
                        q(ROW),
                        term(t, j, ""),
                        if agg.func == AggFn::Min {
                            "ASC"
                        } else {
                            "DESC"
                        },
                        q(ROW)
                    ),
                };
                sel.push(format!("{expr} AS {}", q(&format!("a{a}"))));
            }
            doubles.sort_unstable();
            doubles.dedup();
            // Name the columns registered besides the cells (11.18.2 Where).
            let mut extra = vec![format!(
                "{KEY} = the row's key tuple as canonical JSON (str)"
            )];
            extra.extend(doubles.iter().map(|j| {
                format!(
                    "{} = {:?} as DOUBLE (mean, median)",
                    dbl(*j),
                    t.columns[*j].0
                )
            }));
            let _ = writeln!(sql, "-- {input} also: {}", extra.join(", "));
            let groups: Vec<String> = by.iter().map(|c| q(&col(idx(t, c)))).collect();
            let order: Vec<String> = by
                .iter()
                .map(|c| format!("{} ASC NULLS FIRST", term(t, idx(t, c), "")))
                .collect();
            let _ = write!(
                sql,
                "SELECT {} FROM {from} GROUP BY {} ORDER BY {}",
                sel.join(", "),
                groups.join(", "),
                order.join(", ")
            );
        }
        Op::Join {
            right: _,
            on,
            left_kind,
        } => {
            let (rname, rt) = right?;
            sql.push_str(&header(rname, rt));
            let conds: Vec<String> = on
                .iter()
                .map(|(l, r)| {
                    format!(
                        "l.{} = r.{}",
                        q(&col(idx(t, l))),
                        q(&col(rt.index(r).unwrap_or(usize::MAX)))
                    )
                })
                .collect();
            let _ = write!(
                sql,
                "SELECT l.{} AS {}, r.{} AS {} FROM {from} AS l {} JOIN {} AS r ON {} ORDER BY l.{}",
                q(ROW),
                q("left_row"),
                q(ROW),
                q("right_row"),
                if *left_kind { "LEFT" } else { "INNER" },
                q(rname),
                conds.join(" AND "),
                q(ROW)
            );
        }
    }
    Some(Statement {
        sql,
        key_json,
        doubles,
    })
}

#[cfg(test)]
mod tests {
    use super::COLLATE;

    /// 11.10: the spelling `DuckDB` accepts, and that it orders by bytes;
    /// the unquoted keyword, in either case, is a parser error.
    #[test]
    fn collate_spelling_orders_by_bytes() {
        let e =
            tm_duckdb::open(tm_duckdb::EngineSettings::default()).unwrap_or_else(|e| panic!("{e}"));
        e.execute(
            "CREATE TABLE t(s VARCHAR); INSERT INTO t VALUES ('b'),('B'),('a'),('é'),('e'),('Z');",
        )
        .unwrap_or_else(|e| panic!("{e}"));
        let b = e
            .query(&format!(
                "SELECT string_agg(s, ',' ORDER BY s {COLLATE}) FROM t"
            ))
            .unwrap_or_else(|e| panic!("{e}"));
        let a = b
            .column(0)
            .as_any()
            .downcast_ref::<arrow::array::StringArray>()
            .map(|a| a.value(0).to_owned());
        assert_eq!(a.as_deref(), Some("B,Z,a,b,e,é"));
        assert!(
            e.query("SELECT s FROM t ORDER BY s COLLATE BINARY")
                .is_err()
        );
        assert!(
            e.query("SELECT s FROM t ORDER BY s COLLATE binary")
                .is_err()
        );
        let sha = e
            .query("SELECT sha256('abc')")
            .unwrap_or_else(|e| panic!("{e}"));
        let h = sha
            .column(0)
            .as_any()
            .downcast_ref::<arrow::array::StringArray>()
            .map(|a| a.value(0).to_owned());
        assert_eq!(
            h.as_deref(),
            Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
        );
    }
}
