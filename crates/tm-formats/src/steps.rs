//! Query steps (design 11.8, item 17): the op types, each versioned,
//! structural validation wherever steps are read, and the column and type
//! check against the cited tables' parse specs.
//!
//! Codes (11.8): `E6002` absent or not-allowed choice, unknown op, op
//! version, or key, `take` without an order, join problems, duplicate
//! column; `E6004` wrong JSON type or operand type; `E6005` unknown column;
//! `E6006` expression syntax.

use std::collections::BTreeMap;

use serde_json::{Map, Value};

use crate::errors::{Code, Error, ptr};
use crate::expr::{self, Expr};
use crate::id::VersionHandle;
use crate::parse_spec::ParseSpec;
use crate::schema::{self, Codes};
use crate::typed_value::Tag;
use crate::version;

const CODES: Codes = Codes {
    missing: Code::E6002,
    wrong_type: Code::E6004,
};

/// The version of every gate A op.
pub const OP_VERSION: u64 = 1;

/// `asc` or `desc`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    /// Ascending.
    Asc,
    /// Descending.
    Desc,
}

/// Where missing values go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissingPlace {
    /// First.
    First,
    /// Last.
    Last,
    /// Dropped (`rank` only).
    Exclude,
}

/// `rank` tie rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ties {
    /// SQL `RANK`.
    Min,
    /// The count of rows at or before.
    Max,
    /// SQL `DENSE_RANK`.
    Dense,
    /// `ROW_NUMBER` by value then key.
    Ordinal,
}

/// `group` aggregates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AggFn {
    /// `count`
    Count,
    /// `sum`
    Sum,
    /// `mean`
    Mean,
    /// `min`
    Min,
    /// `max`
    Max,
    /// `median`
    Median,
}

/// One aggregate of a `group`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Aggregate {
    /// Output column.
    pub as_: String,
    /// Function.
    pub func: AggFn,
    /// Input column.
    pub column: String,
}

/// One op, at version 1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    /// `filter`
    Filter {
        /// `where`, parsed.
        where_: Expr,
    },
    /// `select`
    Select {
        /// `columns`.
        columns: Vec<String>,
    },
    /// `sort`
    Sort {
        /// `by`.
        by: Vec<(String, Dir)>,
        /// `missing` (first or last).
        missing: MissingPlace,
    },
    /// `take`
    Take {
        /// `n`.
        n: u64,
    },
    /// `rank`
    Rank {
        /// `by`.
        by: String,
        /// `dir`.
        dir: Dir,
        /// `ties`.
        ties: Ties,
        /// `missing`.
        missing: MissingPlace,
        /// `group_by` when `scope` is `group`; `None` for `all`.
        group_by: Option<Vec<String>>,
    },
    /// `derive`
    Derive {
        /// `as`.
        as_: String,
        /// `expr`, parsed.
        expr: Expr,
    },
    /// `group`
    Group {
        /// `by`.
        by: Vec<String>,
        /// `aggregates`.
        aggregates: Vec<Aggregate>,
    },
    /// `join`
    Join {
        /// `right`.
        right: String,
        /// `on` as (left, right) pairs.
        on: Vec<(String, String)>,
        /// `inner` (`false`) or `left` (`true`).
        left_kind: bool,
    },
}

/// A validated 11.8 steps object.
#[derive(Debug, Clone, PartialEq)]
pub struct Steps {
    /// Table name to version handle, in canonical order.
    pub tables: BTreeMap<String, VersionHandle>,
    /// The start table.
    pub from: String,
    /// The pipeline.
    pub ops: Vec<Op>,
    raw: Value,
}

fn absent_or_empty(at: &str) -> Error {
    Error::at(Code::E6002, at, "a required list is empty")
}

fn names(m: &Map<String, Value>, key: &str, at: &str) -> Result<Vec<String>, Error> {
    let a = schema::array(m, key, at, false, CODES)?;
    if a.is_empty() {
        return Err(absent_or_empty(&ptr(at, key)));
    }
    schema::string_list(m, key, at, true, true, CODES)
}

fn dir(m: &Map<String, Value>, at: &str) -> Result<Dir, Error> {
    Ok(
        if schema::choice(m, "dir", at, &["asc", "desc"], CODES)? == "asc" {
            Dir::Asc
        } else {
            Dir::Desc
        },
    )
}

fn expr_at(m: &Map<String, Value>, key: &str, at: &str) -> Result<Expr, Error> {
    let s = schema::string(m, key, at, CODES)?;
    expr::parse(s, &ptr(at, key))
}

fn op_keys(name: &str, m: &Map<String, Value>) -> Option<&'static [&'static str]> {
    Some(match name {
        "filter" => &["op", "v", "where"],
        "select" => &["op", "v", "columns"],
        "sort" => &["op", "v", "by", "missing"],
        "take" => &["op", "v", "n"],
        "rank" => {
            if m.get("scope").and_then(Value::as_str) == Some("group") {
                &[
                    "op", "v", "by", "dir", "ties", "missing", "scope", "group_by",
                ]
            } else {
                &["op", "v", "by", "dir", "ties", "missing", "scope"]
            }
        }
        "derive" => &["op", "v", "as", "expr"],
        "group" => &["op", "v", "by", "aggregates"],
        "join" => &["op", "v", "right", "on", "kind"],
        _ => return None,
    })
}

#[allow(clippy::too_many_lines, reason = "one arm per 11.8 op")]
fn read_op(
    v: &Value,
    at: &str,
    tables: &BTreeMap<String, VersionHandle>,
    ordered: &mut bool,
) -> Result<Op, Error> {
    let m = schema::object(v, at, CODES, "an op")?;
    if !m.contains_key("op") {
        return Err(Error::at(
            Code::E6002,
            ptr(at, "op"),
            "an op names its `op`",
        ));
    }
    let name = schema::string(m, "op", at, CODES)?;
    let Some(keys) = op_keys(name, m) else {
        return Err(Error::at(
            Code::E6002,
            ptr(at, "op"),
            format!("unknown op {name:?}"),
        ));
    };
    if !m.contains_key("v") {
        return Err(Error::at(
            Code::E6002,
            ptr(at, "v"),
            "an op carries its version `v`",
        ));
    }
    let Some(ver) = m["v"].as_u64() else {
        return Err(Error::at(Code::E6004, ptr(at, "v"), "`v` is an integer"));
    };
    if ver != OP_VERSION {
        return Err(Error::at(
            Code::E6002,
            ptr(at, "v"),
            format!("unknown version {ver} of {name}"),
        ));
    }
    schema::keys(m, at, keys, &[], CODES)?;
    Ok(match name {
        "filter" => Op::Filter {
            where_: expr_at(m, "where", at)?,
        },
        "select" => Op::Select {
            columns: names(m, "columns", at)?,
        },
        "sort" => {
            let a = schema::array(m, "by", at, false, CODES)?;
            if a.is_empty() {
                return Err(absent_or_empty(&ptr(at, "by")));
            }
            let base = ptr(at, "by");
            let mut by = Vec::with_capacity(a.len());
            for (i, e) in a.iter().enumerate() {
                let eat = ptr(&base, i);
                let em = schema::object(e, &eat, CODES, "a sort key")?;
                schema::keys(em, &eat, &["column", "dir"], &[], CODES)?;
                by.push((
                    schema::string(em, "column", &eat, CODES)?.to_owned(),
                    dir(em, &eat)?,
                ));
            }
            let missing = if schema::choice(m, "missing", at, &["first", "last"], CODES)? == "first"
            {
                MissingPlace::First
            } else {
                MissingPlace::Last
            };
            *ordered = true;
            Op::Sort { by, missing }
        }
        "take" => {
            let n = schema::integer(m, "n", at, 0, CODES)?;
            if !*ordered {
                return Err(Error::at(
                    Code::E6002,
                    at,
                    "take needs an earlier sort or rank",
                ));
            }
            Op::Take { n }
        }
        "rank" => {
            let by = schema::string(m, "by", at, CODES)?.to_owned();
            let d = dir(m, at)?;
            let ties =
                match schema::choice(m, "ties", at, &["min", "max", "dense", "ordinal"], CODES)? {
                    "min" => Ties::Min,
                    "max" => Ties::Max,
                    "dense" => Ties::Dense,
                    _ => Ties::Ordinal,
                };
            let missing =
                match schema::choice(m, "missing", at, &["first", "last", "exclude"], CODES)? {
                    "first" => MissingPlace::First,
                    "last" => MissingPlace::Last,
                    _ => MissingPlace::Exclude,
                };
            let scope = schema::choice(m, "scope", at, &["all", "group"], CODES)?;
            let group_by = if scope == "group" {
                Some(names(m, "group_by", at)?)
            } else {
                None
            };
            *ordered = true;
            Op::Rank {
                by,
                dir: d,
                ties,
                missing,
                group_by,
            }
        }
        "derive" => Op::Derive {
            as_: schema::nonempty_string(m, "as", at, CODES)?.to_owned(),
            expr: expr_at(m, "expr", at)?,
        },
        "group" => {
            let by = names(m, "by", at)?;
            let a = schema::array(m, "aggregates", at, false, CODES)?;
            if a.is_empty() {
                return Err(absent_or_empty(&ptr(at, "aggregates")));
            }
            let base = ptr(at, "aggregates");
            let mut aggregates = Vec::with_capacity(a.len());
            for (i, e) in a.iter().enumerate() {
                let eat = ptr(&base, i);
                let em = schema::object(e, &eat, CODES, "an aggregate")?;
                schema::keys(em, &eat, &["as", "fn", "column"], &[], CODES)?;
                let as_ = schema::nonempty_string(em, "as", &eat, CODES)?.to_owned();
                let func = match schema::choice(
                    em,
                    "fn",
                    &eat,
                    &["count", "sum", "mean", "min", "max", "median"],
                    CODES,
                )? {
                    "count" => AggFn::Count,
                    "sum" => AggFn::Sum,
                    "mean" => AggFn::Mean,
                    "min" => AggFn::Min,
                    "max" => AggFn::Max,
                    _ => AggFn::Median,
                };
                let column = schema::string(em, "column", &eat, CODES)?.to_owned();
                aggregates.push(Aggregate { as_, func, column });
            }
            Op::Group { by, aggregates }
        }
        "join" => {
            let right = schema::string(m, "right", at, CODES)?.to_owned();
            if !tables.contains_key(&right) {
                return Err(Error::at(
                    Code::E6002,
                    ptr(at, "right"),
                    format!("no table {right:?} in `tables`"),
                ));
            }
            let a = schema::array(m, "on", at, false, CODES)?;
            if a.is_empty() {
                return Err(absent_or_empty(&ptr(at, "on")));
            }
            let base = ptr(at, "on");
            let mut on = Vec::with_capacity(a.len());
            for (i, e) in a.iter().enumerate() {
                let eat = ptr(&base, i);
                let em = schema::object(e, &eat, CODES, "a join pair")?;
                schema::keys(em, &eat, &["left", "right"], &[], CODES)?;
                on.push((
                    schema::string(em, "left", &eat, CODES)?.to_owned(),
                    schema::string(em, "right", &eat, CODES)?.to_owned(),
                ));
            }
            let left_kind = schema::choice(m, "kind", at, &["inner", "left"], CODES)? == "left";
            Op::Join {
                right,
                on,
                left_kind,
            }
        }
        _ => unreachable!("op_keys accepted only known ops"),
    })
}

impl Steps {
    /// Structural validation: everything 11.8 checks without parse specs.
    ///
    /// # Errors
    /// `E6002`, `E6004`, or `E6006` with `field` at the offending member.
    pub fn from_json(v: &Value) -> Result<Steps, Error> {
        let m = schema::object(v, "", CODES, "steps")?;
        schema::keys(m, "", &["steps", "tables", "from", "ops"], &[], CODES)?;
        match m["steps"].as_u64() {
            Some(version::STEPS) => {}
            Some(n) => {
                return Err(Error::at(
                    Code::E6002,
                    "/steps",
                    format!("unknown steps version {n}"),
                ));
            }
            None => return Err(Error::at(Code::E6004, "/steps", "`steps` is an integer")),
        }
        let Some(tm) = m["tables"].as_object() else {
            return Err(Error::at(Code::E6004, "/tables", "`tables` is an object"));
        };
        if tm.is_empty() {
            return Err(Error::at(
                Code::E6002,
                "/tables",
                "`tables` names at least one table",
            ));
        }
        let mut tables = BTreeMap::new();
        for (name, h) in tm {
            let at = ptr("/tables", name);
            let handle = h
                .as_str()
                .and_then(VersionHandle::parse)
                .ok_or_else(|| Error::at(Code::E6004, &at, "a table is a version handle"))?;
            tables.insert(name.clone(), handle);
        }
        let from = schema::string(m, "from", "", CODES)?.to_owned();
        if !tables.contains_key(&from) {
            return Err(Error::at(
                Code::E6002,
                "/from",
                format!("no table {from:?} in `tables`"),
            ));
        }
        let Some(ops_v) = m["ops"].as_array() else {
            return Err(Error::at(Code::E6004, "/ops", "`ops` is a list"));
        };
        let mut ordered = false;
        let mut ops = Vec::with_capacity(ops_v.len());
        for (i, o) in ops_v.iter().enumerate() {
            ops.push(read_op(o, &ptr("/ops", i), &tables, &mut ordered)?);
        }
        crate::id::id_of(v)?;
        Ok(Steps {
            tables,
            from,
            ops,
            raw: v.clone(),
        })
    }

    /// The object as given.
    #[must_use]
    pub fn to_json(&self) -> &Value {
        &self.raw
    }

    /// Check columns, operand types, and join covers against the tables'
    /// schemas (11.8, 11.18.2). Returns the final table's schema.
    ///
    /// # Errors
    /// `E6005`, `E6004`, or `E6002` with `field` at the offending member.
    pub fn check(&self, schemas: &BTreeMap<String, TableSchema>) -> Result<TableSchema, Error> {
        let schema_of = |name: &str| {
            schemas.get(name).ok_or_else(|| {
                Error::at(
                    Code::E6002,
                    ptr("/tables", name),
                    format!("no parse spec for table {name:?}"),
                )
            })
        };
        let mut cur = schema_of(&self.from)?.clone();
        for (i, op) in self.ops.iter().enumerate() {
            let at = ptr("/ops", i);
            cur = check_op(op, &at, cur, &schema_of)?;
        }
        Ok(cur)
    }
}

/// Columns and key of a table at one point in a pipeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableSchema {
    /// Columns in order with their typed-value tags.
    pub columns: Vec<(String, Tag)>,
    /// Key columns in key order.
    pub key: Vec<String>,
}

impl TableSchema {
    /// A table's schema from its parse spec (`float64` columns are `dec`).
    #[must_use]
    pub fn from_spec(spec: &ParseSpec) -> TableSchema {
        TableSchema {
            columns: spec
                .columns
                .iter()
                .map(|c| (c.name.clone(), c.ty.tag()))
                .collect(),
            key: spec.key.clone(),
        }
    }

    /// The tag of a column.
    #[must_use]
    pub fn tag(&self, name: &str) -> Option<Tag> {
        self.columns
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, t)| *t)
    }

    fn need(&self, name: &str, at: &str) -> Result<Tag, Error> {
        self.tag(name)
            .ok_or_else(|| Error::at(Code::E6005, at, format!("unknown column {name:?}")))
    }

    fn add(&mut self, name: &str, tag: Tag, at: &str) -> Result<(), Error> {
        if self.tag(name).is_some() {
            return Err(Error::at(
                Code::E6002,
                at,
                format!("column {name:?} is already present"),
            ));
        }
        self.columns.push((name.to_owned(), tag));
        Ok(())
    }
}

#[allow(clippy::too_many_lines, reason = "one arm per 11.8 op")]
fn check_op<'s>(
    op: &Op,
    at: &str,
    mut cur: TableSchema,
    schema_of: &dyn Fn(&str) -> Result<&'s TableSchema, Error>,
) -> Result<TableSchema, Error> {
    match op {
        Op::Filter { where_ } => {
            let wat = ptr(at, "where");
            let t = where_.type_of(&|c| cur.tag(c), &wat)?;
            if !matches!(t, None | Some(Tag::Bool)) {
                return Err(Error::at(
                    Code::E6004,
                    wat,
                    format!("`where` gives {t:?}, not a boolean"),
                ));
            }
        }
        Op::Select { columns } => {
            let base = ptr(at, "columns");
            let mut out = Vec::new();
            for (j, c) in columns.iter().enumerate() {
                out.push((c.clone(), cur.need(c, &ptr(&base, j))?));
            }
            for k in &cur.key {
                if !columns.contains(k) {
                    out.push((k.clone(), cur.tag(k).unwrap_or(Tag::Str)));
                }
            }
            cur.columns = out;
        }
        Op::Sort { by, .. } => {
            let base = ptr(at, "by");
            for (j, (c, _)) in by.iter().enumerate() {
                cur.need(c, &ptr(&ptr(&base, j), "column"))?;
            }
        }
        Op::Take { .. } => {}
        Op::Rank { by, group_by, .. } => {
            cur.need(by, &ptr(at, "by"))?;
            if let Some(g) = group_by {
                let base = ptr(at, "group_by");
                for (j, c) in g.iter().enumerate() {
                    cur.need(c, &ptr(&base, j))?;
                }
            }
            cur.add("rank", Tag::Int, at)?;
        }
        Op::Derive { as_, expr } => {
            let eat = ptr(at, "expr");
            let t = expr.type_of(&|c| cur.tag(c), &eat)?;
            let Some(t) = t else {
                return Err(Error::at(
                    Code::E6004,
                    eat,
                    "a derived column needs a type; `missing` alone has none",
                ));
            };
            cur.add(as_, t, &ptr(at, "as"))?;
        }
        Op::Group { by, aggregates } => {
            let base = ptr(at, "by");
            let mut out = TableSchema {
                columns: Vec::new(),
                key: by.clone(),
            };
            for (j, c) in by.iter().enumerate() {
                out.columns.push((c.clone(), cur.need(c, &ptr(&base, j))?));
            }
            let abase = ptr(at, "aggregates");
            for (j, a) in aggregates.iter().enumerate() {
                let aat = ptr(&abase, j);
                let cat = ptr(&aat, "column");
                let t = cur.need(&a.column, &cat)?;
                let numeric = matches!(t, Tag::Int | Tag::Dec);
                let out_t = match a.func {
                    AggFn::Count => Tag::Int,
                    AggFn::Sum if numeric => t,
                    AggFn::Mean | AggFn::Median if numeric => Tag::Dec,
                    AggFn::Min | AggFn::Max if t != Tag::Bool => t,
                    _ => {
                        return Err(Error::at(
                            Code::E6004,
                            cat,
                            format!("{:?} does not take a {t:?} column", a.func),
                        ));
                    }
                };
                out.add(&a.as_, out_t, &ptr(&aat, "as"))?;
            }
            out.add("member_count", Tag::Int, at)?;
            out.add("member_keys_hash", Tag::Str, at)?;
            cur = out;
        }
        Op::Join { right, on, .. } => {
            let rs = schema_of(right)?;
            let base = ptr(at, "on");
            for (j, (l, r)) in on.iter().enumerate() {
                let pat = ptr(&base, j);
                let lt = cur.need(l, &ptr(&pat, "left"))?;
                let rt = rs.need(r, &ptr(&pat, "right"))?;
                if !expr::comparable(Some(lt), Some(rt), true) {
                    return Err(Error::at(
                        Code::E6004,
                        pat,
                        format!("cannot join {lt:?} with {rt:?}"),
                    ));
                }
            }
            for k in &rs.key {
                if !on.iter().any(|(_, r)| r == k) {
                    return Err(Error::at(
                        Code::E6002,
                        base,
                        format!("the pairs do not cover right key column {k:?}"),
                    ));
                }
            }
            for (c, t) in &rs.columns {
                cur.add(&format!("{right}.{c}"), *t, at)?;
            }
        }
    }
    Ok(cur)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn de() -> TableSchema {
        TableSchema {
            columns: vec![
                ("gene_id".into(), Tag::Str),
                ("padj".into(), Tag::Dec),
                ("log2FoldChange".into(), Tag::Dec),
            ],
            key: vec!["gene_id".into()],
        }
    }

    fn err(v: &Value) -> (Code, String) {
        let e = Steps::from_json(v).unwrap_err();
        (e.code, e.field.unwrap_or_default())
    }

    #[test]
    fn valid_pipeline() {
        let v = json!({"steps":1,"tables":{"de":"F7@v2"},"from":"de","ops":[
            {"op":"filter","v":1,"where":"padj < 0.05"},
            {"op":"derive","v":1,"as":"abs_lfc","expr":"abs(log2FoldChange)"},
            {"op":"sort","v":1,"by":[{"column":"abs_lfc","dir":"desc"}],"missing":"last"},
            {"op":"take","v":1,"n":10}
        ]});
        let s = Steps::from_json(&v).unwrap();
        let schemas = BTreeMap::from([("de".to_owned(), de())]);
        let out = s.check(&schemas).unwrap();
        assert_eq!(out.tag("abs_lfc"), Some(Tag::Dec));
    }

    #[test]
    fn refusals() {
        let base = |ops: Value| json!({"steps":1,"tables":{"de":"F7@v2"},"from":"de","ops":ops});
        assert_eq!(
            err(&base(
                json!([{"op":"rank","v":1,"by":"padj","dir":"asc","missing":"last","scope":"all"}])
            )),
            (Code::E6002, "/ops/0/ties".into())
        );
        assert_eq!(
            err(&base(json!([{"op":"take","v":1,"n":3}]))),
            (Code::E6002, "/ops/0".into())
        );
        assert_eq!(
            err(&base(json!([{"op":"filter","v":2,"where":"a"}]))),
            (Code::E6002, "/ops/0/v".into())
        );
        assert_eq!(
            err(&base(json!([{"op":"filter","v":1,"where":"a <"}]))),
            (Code::E6006, "/ops/0/where".into())
        );
        assert_eq!(
            err(&base(json!([{"op":"take","v":1,"n":"3"}]))),
            (Code::E6004, "/ops/0/n".into())
        );
        let v = json!({"steps":1,"tables":{"de":"F7@v2"},"from":"x","ops":[]});
        assert_eq!(err(&v), (Code::E6002, "/from".into()));
        let s =
            Steps::from_json(&base(json!([{"op":"filter","v":1,"where":"gene_id > 1"}]))).unwrap();
        let schemas = BTreeMap::from([("de".to_owned(), de())]);
        let e = s.check(&schemas).unwrap_err();
        assert_eq!(
            (e.code, e.field.unwrap()),
            (Code::E6004, "/ops/0/where".to_owned())
        );
    }
}
