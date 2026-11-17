//! Expression evaluation (design 11.18.2, item 53): `filter.where` and
//! `derive.expr` over the rows of the current table, decimals under the one
//! General Decimal Arithmetic context of 11.6 (precision 34, `Emax` 999999,
//! `Emin` -999999, half-even) with `bigdecimal`.
//!
//! `bigdecimal` applies no context of its own, so every `dec` result is
//! computed exactly and then passed through [`fix`], which is the
//! specification's rounding and range step (Python `decimal`'s `_fix`):
//! round to 34 significant digits, signal Overflow (`E5002`) when the
//! adjusted exponent exceeds `Emax`, and go subnormal, then to zero, below
//! `Emin`, which is not an error. `int` arithmetic is exact and never
//! rounds. A value keeps the exponent the specification gives it, which
//! may be positive after rounding; it is written positionally (11.6) only
//! when it leaves the Checker.

use bigdecimal::BigDecimal;
use bigdecimal::num_bigint::{BigInt, Sign};
use bigdecimal::num_traits::{Signed, Zero};
use std::cmp::Ordering;

use tm_formats::Code;
use tm_formats::errors::Error;
use tm_formats::expr::{BinOp, EMAX, EMIN, Expr, Func, Lit};
use tm_formats::typed_value::{Tag, TypedValue, dec_from_f64_round15, dec_text};

use crate::table::Table;

/// Context precision as an `i64` (11.6).
const PREC: i64 = 34;
/// The smallest exponent a subnormal result may have: `Emin - prec + 1`.
const ETINY: i64 = EMIN - PREC + 1;
/// The largest exponent a full-precision result may have: `Emax - prec + 1`.
const ETOP: i64 = EMAX - PREC + 1;

/// The General Decimal Arithmetic Overflow condition (11.18.2 Domain).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Overflow;

fn pow10(n: u64) -> BigInt {
    // Square-and-multiply; `n` fits a `u32` for every value that can be
    // held in memory.
    let mut result = BigInt::from(1);
    let mut base = BigInt::from(10);
    let mut e = n;
    while e > 0 {
        if e & 1 == 1 {
            result *= &base;
        }
        e >>= 1;
        if e > 0 {
            base = &base * &base;
        }
    }
    result
}

/// Number of decimal digits of `|n|`, 1 for zero.
fn ndigits(n: &BigInt) -> i64 {
    if n.is_zero() {
        return 1;
    }
    let bits = n.bits();
    // Estimate from the bit length, then correct by one comparison.
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "an estimate corrected below"
    )]
    let est = ((bits as f64 - 1.0) * std::f64::consts::LOG10_2).floor() as u64 + 1;
    let m = n.magnitude();
    let est = if m < pow10(est.saturating_sub(1)).magnitude() {
        est - 1
    } else {
        est
    };
    let d = if m >= pow10(est).magnitude() {
        est + 1
    } else {
        est
    };
    i64::try_from(d).unwrap_or(i64::MAX)
}

/// Divide by `10^drop` rounding half-even (`drop` >= 1).
fn round_half_even(coef: &BigInt, drop: u64) -> BigInt {
    let neg = coef.sign() == Sign::Minus;
    let mag = BigInt::from(coef.magnitude().clone());
    let div = pow10(drop);
    let (mut q, r) = (&mag / &div, &mag % &div);
    let twice: BigInt = r * BigInt::from(2);
    match twice.cmp(&div) {
        Ordering::Greater => q += 1,
        Ordering::Equal if (&q % 2u8) == BigInt::from(1) => q += 1,
        _ => {}
    }
    if neg { -q } else { q }
}

/// The specification's rounding and range step for a `dec` result whose
/// exact value is `coef * 10^exp` (Python `decimal`'s `Decimal._fix` with
/// `ROUND_HALF_EVEN` and `clamp=0`).
///
/// # Errors
/// [`Overflow`] when the result's adjusted exponent exceeds `Emax`.
pub fn fix(coef: BigInt, exp: i64) -> Result<BigDecimal, Overflow> {
    if coef.is_zero() {
        let e = exp.clamp(ETINY, EMAX);
        return Ok(BigDecimal::new(coef, -e));
    }
    let len = ndigits(&coef);
    let mut exp_min = len + exp - PREC;
    if exp_min > ETOP {
        return Err(Overflow);
    }
    if exp_min < ETINY {
        exp_min = ETINY;
    }
    if exp >= exp_min {
        return Ok(BigDecimal::new(coef, -exp));
    }
    let drop = u64::try_from(exp_min - exp).unwrap_or(u64::MAX);
    let mut q = if drop > u64::try_from(len).unwrap_or(0) + 1 {
        BigInt::zero()
    } else {
        round_half_even(&coef, drop)
    };
    if !q.is_zero() && ndigits(&q) > PREC {
        q /= 10;
        exp_min += 1;
    }
    if exp_min > ETOP {
        return Err(Overflow);
    }
    Ok(BigDecimal::new(q, -exp_min))
}

fn parts(d: &BigDecimal) -> (BigInt, i64) {
    let (n, scale) = d.as_bigint_and_exponent();
    (n, -scale)
}

fn as_dec(v: &TypedValue) -> Option<BigDecimal> {
    match v {
        TypedValue::Int(n) => Some(BigDecimal::new(n.clone(), 0)),
        TypedValue::Dec(d) => Some(d.clone()),
        _ => None,
    }
}

/// `a + b` (or `a - b` when `negate_b`) in the context.
fn dec_add(a: &BigDecimal, b: &BigDecimal, negate_b: bool) -> Result<BigDecimal, Overflow> {
    let (ca, ea) = parts(a);
    let (mut cb, eb) = parts(b);
    if negate_b {
        cb = -cb;
    }
    let e = ea.min(eb);
    let sa = ca * pow10(u64::try_from(ea - e).unwrap_or(0));
    let sb = cb * pow10(u64::try_from(eb - e).unwrap_or(0));
    fix(sa + sb, e)
}

fn dec_mul(a: &BigDecimal, b: &BigDecimal) -> Result<BigDecimal, Overflow> {
    let (ca, ea) = parts(a);
    let (cb, eb) = parts(b);
    fix(ca * cb, ea + eb)
}

/// General Decimal Arithmetic division at precision 34 (Python
/// `decimal`'s `_divide` then `_fix`). `None` for a zero divisor, which
/// 11.18.2 makes `missing`.
fn dec_div(a: &BigDecimal, b: &BigDecimal) -> Option<Result<BigDecimal, Overflow>> {
    let (ca, ea) = parts(a);
    let (cb, eb) = parts(b);
    if cb.is_zero() {
        return None;
    }
    let ideal = ea - eb;
    if ca.is_zero() {
        return Some(fix(BigInt::zero(), ideal));
    }
    let neg = (ca.sign() == Sign::Minus) != (cb.sign() == Sign::Minus);
    let (ma, mb) = (
        BigInt::from(ca.magnitude().clone()),
        BigInt::from(cb.magnitude().clone()),
    );
    let shift = ndigits(&mb) - ndigits(&ma) + PREC + 1;
    let mut exp = ea - eb - shift;
    let (mut coef, rem) = if shift >= 0 {
        let num = ma * pow10(u64::try_from(shift).unwrap_or(0));
        (&num / &mb, &num % &mb)
    } else {
        let den = mb * pow10(u64::try_from(-shift).unwrap_or(0));
        (&ma / &den, &ma % &den)
    };
    if rem.is_zero() {
        while exp < ideal && (&coef % 10u8).is_zero() {
            coef /= 10;
            exp += 1;
        }
    } else if (&coef % 5u8).is_zero() {
        // The sticky digit: an inexact quotient never ends in 0 or 5, so
        // half-even rounding below sees that it is above the half.
        coef += 1;
    }
    if neg {
        coef = -coef;
    }
    Some(fix(coef, exp))
}

/// A value from an IEEE double result (`log` family): 15 significant
/// digits, `missing` for a non-finite result (11.6, 11.18.2).
fn from_double(x: f64) -> TypedValue {
    dec_from_f64_round15(x).map_or(TypedValue::Missing, TypedValue::Dec)
}

/// The IEEE double nearest a number (correctly rounded from its text).
#[must_use]
pub fn to_double(v: &TypedValue) -> Option<f64> {
    match v {
        TypedValue::Int(n) => n.to_string().parse().ok(),
        TypedValue::Dec(d) => dec_text(d).parse().ok(),
        _ => None,
    }
}

/// True for values every operator treats as `missing` (11.18.2 Missing).
fn is_missing(v: &TypedValue) -> bool {
    matches!(
        v,
        TypedValue::Missing | TypedValue::Nan | TypedValue::Inf(_)
    )
}

/// An expression bound to the current table: columns resolved to
/// indexes, literals to their typed values, and each `coalesce` told
/// whether it widens `int` to `dec`.
#[derive(Debug, Clone)]
pub enum Bound {
    /// A literal's typed value.
    Lit(TypedValue),
    /// A column by index.
    Col(usize),
    /// Unary minus.
    Neg(Box<Bound>),
    /// `not`.
    Not(Box<Bound>),
    /// A binary operator.
    Bin(BinOp, Box<Bound>, Box<Bound>),
    /// A call; the flag is set on a `coalesce` whose common type is `dec`.
    Call(Func, Vec<Bound>, bool),
}

/// Bind an expression that has passed the 11.8 type check to `table`.
///
/// # Errors
/// `E6005` for a column the table does not have, `E6004` for a literal
/// that has no value (validation refuses both first).
pub fn bind(e: &Expr, table: &Table, at: &str) -> Result<Bound, Error> {
    let tag = |c: &str| table.tag(c);
    Ok(match e {
        Expr::Lit(l) => Bound::Lit(literal(l, at)?),
        Expr::Col(c) => Bound::Col(
            table
                .index(c)
                .ok_or_else(|| Error::at(Code::E6005, at, format!("unknown column {c:?}")))?,
        ),
        Expr::Neg(x) => Bound::Neg(Box::new(bind(x, table, at)?)),
        Expr::Not(x) => Bound::Not(Box::new(bind(x, table, at)?)),
        Expr::Bin(op, l, r) => Bound::Bin(
            *op,
            Box::new(bind(l, table, at)?),
            Box::new(bind(r, table, at)?),
        ),
        Expr::Call(f, args) => {
            let widen = *f == Func::Coalesce && e.type_of(&tag, at)? == Some(Tag::Dec);
            Bound::Call(
                *f,
                args.iter()
                    .map(|a| bind(a, table, at))
                    .collect::<Result<_, _>>()?,
                widen,
            )
        }
    })
}

/// A literal's typed value: converted exactly, outside the context, to its
/// positional form (11.18.1; rulings 45 and 48).
fn literal(l: &Lit, at: &str) -> Result<TypedValue, Error> {
    l.value()
        .ok_or_else(|| Error::at(Code::E6004, at, "the literal has no value"))
}

fn overflow(at: &str) -> Error {
    Error::at(
        Code::E5002,
        at,
        "arithmetic overflow: the result's adjusted exponent exceeds Emax 999999",
    )
}

fn cmp_values(a: &TypedValue, b: &TypedValue) -> Option<Ordering> {
    match (a, b) {
        (TypedValue::Int(_) | TypedValue::Dec(_), TypedValue::Int(_) | TypedValue::Dec(_)) => {
            Some(as_dec(a)?.cmp(&as_dec(b)?))
        }
        (TypedValue::Str(x), TypedValue::Str(y)) => Some(x.as_bytes().cmp(y.as_bytes())),
        (TypedValue::Bool(x), TypedValue::Bool(y)) => Some(x.cmp(y)),
        (TypedValue::Date(x), TypedValue::Date(y)) => Some(x.cmp(y)),
        (TypedValue::DateTime(x), TypedValue::DateTime(y)) => Some(x.cmp(y)),
        _ => None,
    }
}

impl Bound {
    /// Evaluate on row `row` of `table`.
    ///
    /// # Errors
    /// `E5002` at `at` on Overflow; `E6004` at `at` for an operand pair the
    /// type check should have refused.
    pub fn eval(&self, table: &Table, row: usize, at: &str) -> Result<TypedValue, Error> {
        let rows = &table.rows;
        Ok(match self {
            Bound::Lit(v) => v.clone(),
            Bound::Col(i) => rows[row][*i].clone(),
            Bound::Neg(x) => {
                let v = x.eval(table, row, at)?;
                match v {
                    _ if is_missing(&v) => TypedValue::Missing,
                    TypedValue::Int(n) => TypedValue::Int(-n),
                    TypedValue::Dec(d) => {
                        let (c, e) = parts(&d);
                        TypedValue::Dec(fix(-c, e).map_err(|_| overflow(at))?)
                    }
                    other => return Err(type_error(at, "unary minus", &other)),
                }
            }
            Bound::Not(x) => match x.eval(table, row, at)? {
                TypedValue::Bool(b) => TypedValue::Bool(!b),
                v if is_missing(&v) => TypedValue::Missing,
                other => return Err(type_error(at, "not", &other)),
            },
            Bound::Bin(op, l, r) => self.binary(*op, l, r, table, row, at)?,
            Bound::Call(f, args, widen) => call(*f, args, *widen, table, row, at)?,
        })
    }

    fn binary(
        &self,
        op: BinOp,
        l: &Bound,
        r: &Bound,
        table: &Table,
        row: usize,
        at: &str,
    ) -> Result<TypedValue, Error> {
        match op {
            BinOp::And | BinOp::Or => {
                let a = l.eval(table, row, at)?;
                let b = r.eval(table, row, at)?;
                let truth = |v: &TypedValue| -> Result<Option<bool>, Error> {
                    match v {
                        TypedValue::Bool(b) => Ok(Some(*b)),
                        v if is_missing(v) => Ok(None),
                        other => Err(type_error(at, "and/or", other)),
                    }
                };
                let (a, b) = (truth(&a)?, truth(&b)?);
                let out = if op == BinOp::And {
                    match (a, b) {
                        (Some(false), _) | (_, Some(false)) => Some(false),
                        (Some(true), Some(true)) => Some(true),
                        _ => None,
                    }
                } else {
                    match (a, b) {
                        (Some(true), _) | (_, Some(true)) => Some(true),
                        (Some(false), Some(false)) => Some(false),
                        _ => None,
                    }
                };
                Ok(out.map_or(TypedValue::Missing, TypedValue::Bool))
            }
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                let a = l.eval(table, row, at)?;
                let b = r.eval(table, row, at)?;
                if is_missing(&a) || is_missing(&b) {
                    return Ok(TypedValue::Missing);
                }
                let o = cmp_values(&a, &b).ok_or_else(|| {
                    Error::at(
                        Code::E6004,
                        at,
                        format!("{:?} does not compare with {:?}", a.tag(), b.tag()),
                    )
                })?;
                Ok(TypedValue::Bool(match op {
                    BinOp::Eq => o == Ordering::Equal,
                    BinOp::Ne => o != Ordering::Equal,
                    BinOp::Lt => o == Ordering::Less,
                    BinOp::Le => o != Ordering::Greater,
                    BinOp::Gt => o == Ordering::Greater,
                    _ => o != Ordering::Less,
                }))
            }
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div => {
                let a = l.eval(table, row, at)?;
                let b = r.eval(table, row, at)?;
                if is_missing(&a) || is_missing(&b) {
                    return Ok(TypedValue::Missing);
                }
                arith(op, &a, &b, at)
            }
        }
    }
}

fn type_error(at: &str, what: &str, v: &TypedValue) -> Error {
    Error::at(Code::E6004, at, format!("{what} on {:?}", v.tag()))
}

/// `+`, `-`, `*`, `/` on two present numbers (11.18.2 Types, Precision,
/// Domain).
///
/// # Errors
/// `E5002` on Overflow, `E6004` on a non-number.
pub fn arith(op: BinOp, a: &TypedValue, b: &TypedValue, at: &str) -> Result<TypedValue, Error> {
    if let (TypedValue::Int(x), TypedValue::Int(y)) = (a, b) {
        match op {
            BinOp::Add => return Ok(TypedValue::Int(x + y)),
            BinOp::Sub => return Ok(TypedValue::Int(x - y)),
            BinOp::Mul => return Ok(TypedValue::Int(x * y)),
            _ => {}
        }
    }
    let (Some(x), Some(y)) = (as_dec(a), as_dec(b)) else {
        return Err(Error::at(
            Code::E6004,
            at,
            format!("arithmetic on {:?} and {:?}", a.tag(), b.tag()),
        ));
    };
    let r = match op {
        BinOp::Add => dec_add(&x, &y, false),
        BinOp::Sub => dec_add(&x, &y, true),
        BinOp::Mul => dec_mul(&x, &y),
        _ => match dec_div(&x, &y) {
            Some(r) => r,
            None => return Ok(TypedValue::Missing),
        },
    };
    r.map(TypedValue::Dec).map_err(|_| overflow(at))
}

fn call(
    f: Func,
    args: &[Bound],
    widen: bool,
    table: &Table,
    row: usize,
    at: &str,
) -> Result<TypedValue, Error> {
    if f == Func::Coalesce {
        for a in args {
            let v = a.eval(table, row, at)?;
            if is_missing(&v) {
                continue;
            }
            return Ok(match v {
                TypedValue::Int(n) if widen => TypedValue::Dec(BigDecimal::new(n, 0)),
                other => other,
            });
        }
        return Ok(TypedValue::Missing);
    }
    let v = args[0].eval(table, row, at)?;
    if is_missing(&v) {
        return Ok(TypedValue::Missing);
    }
    match f {
        Func::Abs => Ok(match v {
            TypedValue::Int(n) => TypedValue::Int(n.abs()),
            TypedValue::Dec(d) => {
                let (c, e) = parts(&d);
                TypedValue::Dec(fix(c.abs(), e).map_err(|_| overflow(at))?)
            }
            other => return Err(type_error(at, "abs", &other)),
        }),
        Func::Log | Func::Log2 | Func::Log10 => {
            let d = as_dec(&v).ok_or_else(|| type_error(at, "log", &v))?;
            if d <= BigDecimal::zero() {
                return Ok(TypedValue::Missing);
            }
            let x = to_double(&v).unwrap_or(f64::NAN);
            Ok(from_double(match f {
                Func::Log => x.ln(),
                Func::Log2 => x.log2(),
                _ => x.log10(),
            }))
        }
        Func::Exp => {
            let x = to_double(&v).ok_or_else(|| type_error(at, "exp", &v))?;
            Ok(from_double(x.exp()))
        }
        Func::Coalesce => unreachable!("handled above"),
    }
}

/// The exact sum of present numbers in the context, in member order, for
/// the `group` aggregate `sum`: `int` stays exact, `dec` rounds after each
/// addition as General Decimal Arithmetic does. `missing` when no member
/// is present (11.18.2 `group`).
///
/// # Errors
/// `E5002` at `at` on Overflow.
pub fn sum<'a>(
    values: impl Iterator<Item = &'a TypedValue>,
    tag: Tag,
    at: &str,
) -> Result<TypedValue, Error> {
    let mut acc: Option<TypedValue> = None;
    for v in values {
        if is_missing(v) {
            continue;
        }
        acc = Some(match acc {
            None => match (tag, v) {
                (Tag::Dec, TypedValue::Int(n)) => TypedValue::Dec(BigDecimal::new(n.clone(), 0)),
                _ => v.clone(),
            },
            Some(a) => arith(BinOp::Add, &a, v, at)?,
        });
    }
    Ok(acc.unwrap_or(TypedValue::Missing))
}

/// The double of a value for `mean` and `median` in `DuckDB`.
#[must_use]
pub fn double_or_none(v: &TypedValue) -> Option<f64> {
    if is_missing(v) { None } else { to_double(v) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tm_formats::typed_value::parse_dec_text;

    fn d(s: &str) -> BigDecimal {
        parse_dec_text(s).unwrap()
    }

    fn t(r: Result<BigDecimal, Overflow>) -> String {
        dec_text(&r.unwrap())
    }

    #[test]
    fn division_is_python_decimal() {
        assert_eq!(t(dec_div(&d("10"), &d("4")).unwrap()), "2.5");
        assert_eq!(t(dec_div(&d("4"), &d("4")).unwrap()), "1");
        assert_eq!(t(dec_div(&d("3"), &d("4")).unwrap()), "0.75");
        assert_eq!(t(dec_div(&d("6.0"), &d("2")).unwrap()), "3.0");
        assert_eq!(
            t(dec_div(&d("2"), &d("3")).unwrap()),
            "0.6666666666666666666666666666666667"
        );
        assert_eq!(
            t(dec_div(&d("7"), &d("3")).unwrap()),
            "2.333333333333333333333333333333333"
        );
        assert_eq!(t(dec_div(&d("0.00"), &d("4")).unwrap()), "0.00");
        assert!(dec_div(&d("1"), &d("0")).is_none());
    }

    #[test]
    fn rounding_and_range() {
        assert_eq!(
            t(dec_mul(
                &d("10000000000000000.5"),
                &d("10000000000000000.5")
            )),
            "100000000000000010000000000000000.2"
        );
        assert_eq!(t(dec_mul(&d("1.10"), &d("2"))), "2.20");
        let big = BigDecimal::new(BigInt::from(99), -999_998);
        assert!(dec_mul(&big, &d("10")).is_err());
        assert!(dec_mul(&big, &d("1")).is_ok());
        // Underflow: subnormal, then zero, not an error.
        let tiny = BigDecimal::new(BigInt::from(1), 999_999);
        let r = dec_mul(&tiny, &d("0.1")).unwrap();
        assert_eq!(parts(&r), (BigInt::from(1), -1_000_000));
        let z = dec_mul(&BigDecimal::new(BigInt::from(1), 1_000_032), &d("0.1")).unwrap();
        assert!(z.is_zero());
        assert_eq!(parts(&z).1, ETINY);
    }

    #[test]
    fn digits() {
        for s in [
            "1",
            "9",
            "10",
            "99",
            "100",
            "123456789012345678901234567890",
        ] {
            let n: BigInt = s.parse().unwrap();
            assert_eq!(ndigits(&n), i64::try_from(s.len()).unwrap(), "{s}");
        }
        assert_eq!(ndigits(&pow10(1000)), 1001);
        assert_eq!(ndigits(&(pow10(1000) - 1)), 1000);
    }
}
