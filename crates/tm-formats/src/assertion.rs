//! Assertions (design 11.9 and 11.18.3, item 18): the schema and the
//! comparison rules as pure functions on typed values.
//!
//! Locating rows in the final table is the Checker's (P4); here a kind is
//! evaluated against what was located: the cell or cells (or their
//! absence), the row's 1-based position, the row count, or presence.

use bigdecimal::BigDecimal;
use serde_json::{Map, Value};

use crate::errors::{Code, Error, ptr};
use crate::schema::{self, Codes};
use crate::typed_value::{Tag, TypedValue, parse_dec_text};
use crate::version;

const CODES: Codes = Codes {
    missing: Code::E6002,
    wrong_type: Code::E6004,
};

/// A comparison operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cmp {
    /// `eq`
    Eq,
    /// `ne`
    Ne,
    /// `lt`
    Lt,
    /// `le`
    Le,
    /// `gt`
    Gt,
    /// `ge`
    Ge,
    /// `between`
    Between,
}

impl Cmp {
    fn parse(s: &str) -> Cmp {
        match s {
            "eq" => Cmp::Eq,
            "ne" => Cmp::Ne,
            "lt" => Cmp::Lt,
            "le" => Cmp::Le,
            "gt" => Cmp::Gt,
            "ge" => Cmp::Ge,
            _ => Cmp::Between,
        }
    }
}

const OPS: &[&str] = &["eq", "ne", "lt", "le", "gt", "ge", "between"];
const OPS_NO_BETWEEN: &[&str] = &["eq", "ne", "lt", "le", "gt", "ge"];

/// A tolerance: `m = abs + rel * |b|`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tol {
    /// Absolute part.
    pub abs: BigDecimal,
    /// Relative part.
    pub rel: BigDecimal,
}

impl Tol {
    fn margin(&self, b: &BigDecimal) -> BigDecimal {
        &self.abs + &self.rel * b.abs()
    }
}

/// `expected`: one value, or `[lo, hi]` for `between`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expected {
    /// One value.
    One(TypedValue),
    /// `[lo, hi]`.
    Range(TypedValue, TypedValue),
}

/// A located cell: a row's key values and a column.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellRef {
    /// Key values in the final table's key order.
    pub row: Vec<TypedValue>,
    /// Column name.
    pub column: String,
}

/// A validated 11.9 assertion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Assertion {
    /// `value`
    Value {
        /// The cell.
        cell: CellRef,
        /// Operator.
        op: Cmp,
        /// Expected value.
        expected: Expected,
        /// Tolerance.
        tol: Option<Tol>,
    },
    /// `rank`
    Rank {
        /// Key values.
        row: Vec<TypedValue>,
        /// Expected 1-based position.
        expected: TypedValue,
        /// Tolerance.
        tol: Option<Tol>,
    },
    /// `count`
    Count {
        /// Operator.
        op: Cmp,
        /// Expected count.
        expected: Expected,
    },
    /// `compare_rows`
    CompareRows {
        /// Left cell.
        left: CellRef,
        /// Right cell.
        right: CellRef,
        /// Operator (not `between`).
        op: Cmp,
        /// Tolerance.
        tol: Option<Tol>,
    },
    /// `exists`
    Exists {
        /// Key values.
        row: Vec<TypedValue>,
    },
}

/// The outcome of an assertion that could be evaluated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// `holds`
    Holds,
    /// `fails`
    Fails,
}

impl Outcome {
    /// The 11.16 spelling.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Outcome::Holds => "holds",
            Outcome::Fails => "fails",
        }
    }

    fn of(b: bool) -> Outcome {
        if b { Outcome::Holds } else { Outcome::Fails }
    }
}

/// What the Checker located for an assertion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Observed<'a> {
    /// `value`: the cell, or `None` if the row is absent.
    Cell(Option<&'a TypedValue>),
    /// `rank`: the row's 1-based position, or `None` if absent.
    Position(Option<u64>),
    /// `count`: the final row count.
    Count(u64),
    /// `compare_rows`: the two cells, each `None` if its row is absent.
    Cells(Option<&'a TypedValue>, Option<&'a TypedValue>),
    /// `exists`: whether the row is present.
    Present(bool),
}

fn typed(v: &Value, at: &str) -> Result<TypedValue, Error> {
    TypedValue::from_json(v, at)
}

fn row(m: &Map<String, Value>, key: &str, at: &str) -> Result<Vec<TypedValue>, Error> {
    let a = schema::array(m, key, at, false, CODES)?;
    let base = ptr(at, key);
    if a.is_empty() {
        return Err(Error::at(
            Code::E6002,
            base,
            "a row names at least one key value",
        ));
    }
    a.iter()
        .enumerate()
        .map(|(i, v)| typed(v, &ptr(&base, i)))
        .collect()
}

fn cell(m: &Map<String, Value>, key: &str) -> Result<CellRef, Error> {
    let at = ptr("", key);
    let cm = schema::object(&m[key], &at, CODES, "a cell")?;
    schema::keys(cm, &at, &["row", "column"], &[], CODES)?;
    Ok(CellRef {
        row: row(cm, "row", &at)?,
        column: schema::string(cm, "column", &at, CODES)?.to_owned(),
    })
}

fn tol(m: &Map<String, Value>) -> Result<Option<Tol>, Error> {
    let Some(v) = m.get("tol") else {
        return Ok(None);
    };
    let tm = schema::object(v, "/tol", CODES, "`tol`")?;
    schema::keys(tm, "/tol", &["abs", "rel"], &[], CODES)?;
    let part = |k: &str| -> Result<BigDecimal, Error> {
        let s = schema::string(tm, k, "/tol", CODES)?;
        parse_dec_text(s)
            .filter(|d| d >= &BigDecimal::from(0))
            .ok_or_else(|| {
                Error::at(
                    Code::E6004,
                    ptr("/tol", k),
                    format!("{s:?} is not a non-negative decimal"),
                )
            })
    };
    Ok(Some(Tol {
        abs: part("abs")?,
        rel: part("rel")?,
    }))
}

fn comparable_expected(v: &TypedValue, at: &str) -> Result<(), Error> {
    if matches!(v.tag(), Tag::Missing | Tag::Nan | Tag::Inf | Tag::List) {
        return Err(Error::at(
            Code::E6004,
            at,
            format!("`expected` cannot be {:?}", v.tag()),
        ));
    }
    Ok(())
}

fn expected(m: &Map<String, Value>, op: Cmp) -> Result<Expected, Error> {
    let v = &m["expected"];
    if op == Cmp::Between {
        let Some(a) = v.as_array().filter(|a| a.len() == 2) else {
            return Err(Error::at(
                Code::E6004,
                "/expected",
                "`between` expects a two-element list [lo, hi]",
            ));
        };
        let lo = typed(&a[0], "/expected/0")?;
        let hi = typed(&a[1], "/expected/1")?;
        comparable_expected(&lo, "/expected/0")?;
        comparable_expected(&hi, "/expected/1")?;
        if lo.compare(&hi).is_none() {
            return Err(Error::at(
                Code::E6004,
                "/expected/1",
                "`lo` and `hi` do not compare",
            ));
        }
        Ok(Expected::Range(lo, hi))
    } else {
        let e = typed(v, "/expected")?;
        comparable_expected(&e, "/expected")?;
        Ok(Expected::One(e))
    }
}

fn is_numeric(t: Tag) -> bool {
    matches!(t, Tag::Int | Tag::Dec)
}

/// Static checks of operator and tolerance against the expected value's
/// type (11.18.3).
fn check_static(op: Cmp, exp: &Expected, tol: Option<&Tol>) -> Result<(), Error> {
    let t = match exp {
        Expected::One(v) | Expected::Range(v, _) => v.tag(),
    };
    if !is_numeric(t) && tol.is_some() {
        return Err(Error::at(
            Code::E6004,
            "/tol",
            format!("`tol` does not apply to {t:?}"),
        ));
    }
    if matches!(t, Tag::Str | Tag::Bool) && !matches!(op, Cmp::Eq | Cmp::Ne) {
        return Err(Error::at(
            Code::E6004,
            "/op",
            format!("{t:?} values take only eq and ne"),
        ));
    }
    Ok(())
}

fn int_expected(e: &TypedValue, at: &str) -> Result<(), Error> {
    if e.tag() != Tag::Int {
        return Err(Error::at(Code::E6004, at, "`expected` is an int here"));
    }
    Ok(())
}

impl Assertion {
    /// Validate an 11.9 object.
    ///
    /// # Errors
    /// `E6002`, `E6004`, or (for a malformed typed value) `E1004`, with
    /// `field` at the offending member.
    pub fn from_json(v: &Value) -> Result<Assertion, Error> {
        let m = schema::object(v, "", CODES, "an assertion")?;
        for k in ["assert", "kind"] {
            if !m.contains_key(k) {
                return Err(Error::at(
                    Code::E6002,
                    ptr("", k),
                    format!("required member `{k}` is absent"),
                ));
            }
        }
        match m["assert"].as_u64() {
            Some(version::ASSERTION) => {}
            Some(n) => {
                return Err(Error::at(
                    Code::E6002,
                    "/assert",
                    format!("unknown assertion version {n}"),
                ));
            }
            None => return Err(Error::at(Code::E6004, "/assert", "`assert` is an integer")),
        }
        let kind = schema::choice(
            m,
            "kind",
            "",
            &["value", "rank", "count", "compare_rows", "exists"],
            CODES,
        )?;
        let (required, optional): (&[&str], &[&str]) = match kind {
            "value" => (
                &["assert", "kind", "row", "column", "op", "expected"],
                &["tol"],
            ),
            "rank" => (&["assert", "kind", "row", "expected"], &["tol"]),
            "count" => (&["assert", "kind", "op", "expected"], &[]),
            "compare_rows" => (&["assert", "kind", "left", "right", "op"], &["tol"]),
            _ => (&["assert", "kind", "row"], &[]),
        };
        schema::keys(m, "", required, optional, CODES)?;
        let a = match kind {
            "value" => {
                let r = row(m, "row", "")?;
                let column = schema::string(m, "column", "", CODES)?.to_owned();
                let op = Cmp::parse(schema::choice(m, "op", "", OPS, CODES)?);
                let exp = expected(m, op)?;
                let t = tol(m)?;
                check_static(op, &exp, t.as_ref())?;
                Assertion::Value {
                    cell: CellRef { row: r, column },
                    op,
                    expected: exp,
                    tol: t,
                }
            }
            "rank" => {
                let r = row(m, "row", "")?;
                let Expected::One(e) = expected(m, Cmp::Eq)? else {
                    unreachable!("eq gives one value")
                };
                int_expected(&e, "/expected")?;
                Assertion::Rank {
                    row: r,
                    expected: e,
                    tol: tol(m)?,
                }
            }
            "count" => {
                let op = Cmp::parse(schema::choice(m, "op", "", OPS, CODES)?);
                let exp = expected(m, op)?;
                match &exp {
                    Expected::One(e) => int_expected(e, "/expected")?,
                    Expected::Range(lo, hi) => {
                        int_expected(lo, "/expected/0")?;
                        int_expected(hi, "/expected/1")?;
                    }
                }
                Assertion::Count { op, expected: exp }
            }
            "compare_rows" => {
                let left = cell(m, "left")?;
                let right = cell(m, "right")?;
                let op = Cmp::parse(schema::choice(m, "op", "", OPS_NO_BETWEEN, CODES)?);
                Assertion::CompareRows {
                    left,
                    right,
                    op,
                    tol: tol(m)?,
                }
            }
            _ => Assertion::Exists {
                row: row(m, "row", "")?,
            },
        };
        crate::id::id_of(v)?;
        Ok(a)
    }

    /// Evaluate against what was located (11.18.3).
    ///
    /// # Errors
    /// `E6004` for a comparison between types 11.18.3 does not allow
    /// (with `field` at the member that fixes the type), and `E5001` if
    /// `obs` is not the observation this kind takes.
    pub fn evaluate(&self, obs: Observed<'_>) -> Result<Outcome, Error> {
        match (self, obs) {
            (
                Assertion::Value {
                    op, expected, tol, ..
                },
                Observed::Cell(c),
            ) => {
                let Some(a) = c else {
                    return Ok(Outcome::Fails);
                };
                compare(a, *op, expected, tol.as_ref(), "/expected")
            }
            (Assertion::Rank { expected, tol, .. }, Observed::Position(p)) => {
                let Some(p) = p else {
                    return Ok(Outcome::Fails);
                };
                let a = TypedValue::Int(p.into());
                compare(
                    &a,
                    Cmp::Eq,
                    &Expected::One(expected.clone()),
                    tol.as_ref(),
                    "/expected",
                )
            }
            (Assertion::Count { op, expected }, Observed::Count(n)) => {
                compare(&TypedValue::Int(n.into()), *op, expected, None, "/expected")
            }
            (Assertion::CompareRows { op, tol, .. }, Observed::Cells(l, r)) => {
                let (Some(a), Some(b)) = (l, r) else {
                    return Ok(Outcome::Fails);
                };
                if never_compares(b) {
                    return Ok(Outcome::Fails);
                }
                compare(a, *op, &Expected::One(b.clone()), tol.as_ref(), "/right")
            }
            (Assertion::Exists { .. }, Observed::Present(p)) => Ok(Outcome::of(p)),
            _ => Err(Error::new(
                Code::E5001,
                "observation does not match the assertion kind",
            )),
        }
    }
}

fn never_compares(v: &TypedValue) -> bool {
    matches!(
        v,
        TypedValue::Missing | TypedValue::Nan | TypedValue::Inf(_)
    )
}

#[allow(
    clippy::many_single_char_names,
    reason = "a, b, m, x, y are 11.18.3's names"
)]
fn compare(
    a: &TypedValue,
    op: Cmp,
    exp: &Expected,
    tol: Option<&Tol>,
    at: &str,
) -> Result<Outcome, Error> {
    if never_compares(a) {
        return Ok(Outcome::Fails);
    }
    let b = match exp {
        Expected::One(b) | Expected::Range(b, _) => b,
    };
    let (ta, tb) = (a.tag(), b.tag());
    let numeric = is_numeric(ta) && is_numeric(tb);
    if !numeric && ta != tb {
        return Err(Error::at(
            Code::E6004,
            at,
            format!("{ta:?} does not compare with {tb:?}"),
        ));
    }
    if !numeric {
        if tol.is_some() {
            return Err(Error::at(
                Code::E6004,
                "/tol",
                format!("`tol` does not apply to {ta:?}"),
            ));
        }
        if matches!(ta, Tag::Str | Tag::Bool) && !matches!(op, Cmp::Eq | Cmp::Ne) {
            return Err(Error::at(
                Code::E6004,
                "/op",
                format!("{ta:?} values take only eq and ne"),
            ));
        }
        if matches!(ta, Tag::List) {
            return Err(Error::at(Code::E6004, at, "a list does not compare"));
        }
        let ord = |x: &TypedValue, y: &TypedValue| {
            x.compare(y)
                .ok_or_else(|| Error::at(Code::E6004, at, "values do not compare"))
        };
        return Ok(Outcome::of(match (op, exp) {
            (Cmp::Between, Expected::Range(lo, hi)) => ord(a, lo)?.is_ge() && ord(a, hi)?.is_le(),
            (Cmp::Eq, _) => ord(a, b)?.is_eq(),
            (Cmp::Ne, _) => ord(a, b)?.is_ne(),
            (Cmp::Lt, _) => ord(a, b)?.is_lt(),
            (Cmp::Le, _) => ord(a, b)?.is_le(),
            (Cmp::Gt, _) => ord(a, b)?.is_gt(),
            (Cmp::Ge, _) => ord(a, b)?.is_ge(),
            (Cmp::Between, Expected::One(_)) => false,
        }));
    }
    let zero = Tol {
        abs: BigDecimal::from(0),
        rel: BigDecimal::from(0),
    };
    let t = tol.unwrap_or(&zero);
    let x = a.as_decimal().unwrap_or_default();
    let dec = |v: &TypedValue| {
        v.as_decimal()
            .ok_or_else(|| Error::at(Code::E6004, at, "not a number"))
    };
    Ok(Outcome::of(match (op, exp) {
        (Cmp::Between, Expected::Range(lo, hi)) => {
            let (lo, hi) = (dec(lo)?, dec(hi)?);
            let lo_m = t.margin(&lo);
            let hi_m = t.margin(&hi);
            lo - lo_m <= x && x <= hi + hi_m
        }
        (Cmp::Between, Expected::One(_)) => false,
        (op, _) => {
            let y = dec(b)?;
            let m = t.margin(&y);
            match op {
                Cmp::Eq => (&x - &y).abs() <= m,
                Cmp::Ne => (&x - &y).abs() > m,
                Cmp::Lt => x < y + m,
                Cmp::Le => x <= y + m,
                Cmp::Gt => x > y - m,
                _ => x >= y - m,
            }
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn dec(s: &str) -> TypedValue {
        TypedValue::Dec(parse_dec_text(s).unwrap())
    }

    #[test]
    fn value_with_tolerance() {
        let a = Assertion::from_json(&json!({"assert":1,"kind":"value","row":[{"t":"str","v":"TP53"}],"column":"padj","op":"eq","expected":{"t":"dec","v":"0.0031"},"tol":{"abs":"0.0001","rel":"0"}})).unwrap();
        assert_eq!(
            a.evaluate(Observed::Cell(Some(&dec("0.00315")))).unwrap(),
            Outcome::Holds
        );
        assert_eq!(
            a.evaluate(Observed::Cell(Some(&dec("0.0033")))).unwrap(),
            Outcome::Fails
        );
        assert_eq!(
            a.evaluate(Observed::Cell(Some(&TypedValue::Missing)))
                .unwrap(),
            Outcome::Fails
        );
        assert_eq!(a.evaluate(Observed::Cell(None)).unwrap(), Outcome::Fails);
        assert_eq!(
            a.evaluate(Observed::Cell(Some(&TypedValue::Str("x".into()))))
                .unwrap_err()
                .code,
            Code::E6004
        );
    }

    #[test]
    fn between_and_ordering() {
        let a = Assertion::from_json(&json!({"assert":1,"kind":"count","op":"between","expected":[{"t":"int","v":"5"},{"t":"int","v":"10"}]})).unwrap();
        assert_eq!(a.evaluate(Observed::Count(5)).unwrap(), Outcome::Holds);
        assert_eq!(a.evaluate(Observed::Count(11)).unwrap(), Outcome::Fails);
        let a = Assertion::from_json(&json!({"assert":1,"kind":"value","row":[{"t":"int","v":"1"}],"column":"x","op":"lt","expected":{"t":"dec","v":"1.0"},"tol":{"abs":"0.1","rel":"0"}})).unwrap();
        assert_eq!(
            a.evaluate(Observed::Cell(Some(&dec("1.05")))).unwrap(),
            Outcome::Holds
        );
        assert_eq!(
            a.evaluate(Observed::Cell(Some(&dec("1.1")))).unwrap(),
            Outcome::Fails
        );
    }

    #[test]
    fn refusals() {
        let e = |v: Value| {
            let e = Assertion::from_json(&v).unwrap_err();
            (e.code, e.field.unwrap_or_default())
        };
        assert_eq!(
            e(json!({"assert":1,"kind":"count","op":"eq"})),
            (Code::E6002, "/expected".into())
        );
        assert_eq!(
            e(
                json!({"assert":1,"kind":"count","op":"eq","expected":{"t":"int","v":"1"},"tol":{"abs":"0","rel":"0"}})
            ),
            (Code::E6002, "/tol".into())
        );
        assert_eq!(
            e(
                json!({"assert":1,"kind":"value","row":[{"t":"int","v":"1"}],"column":"g","op":"lt","expected":{"t":"str","v":"a"}})
            ),
            (Code::E6004, "/op".into())
        );
        assert_eq!(
            e(
                json!({"assert":1,"kind":"compare_rows","left":{"row":[{"t":"int","v":"1"}],"column":"a"},"right":{"row":[{"t":"int","v":"2"}],"column":"a"},"op":"between"})
            ),
            (Code::E6002, "/op".into())
        );
        assert_eq!(
            e(
                json!({"assert":1,"kind":"value","row":[{"t":"int","v":"1"}],"column":"g","op":"eq","expected":{"t":"dec","v":"1e3"}})
            ),
            (Code::E1004, "/expected/v".into())
        );
    }
}
