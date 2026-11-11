//! Typed values (design 11.6, item 15): the one encoding used in events,
//! steps, assertions, and Arrow conversion.
//!
//! Decimals are `bigdecimal` values that keep the scale as written. The
//! text form is always positional, never an exponent, and never carries a
//! minus sign on zero. Arithmetic in `derive` is the Checker's (P4); this
//! module holds the encoding, its lexical rules (`E1004`), the two
//! IEEE-double conversions 11.6 names, the 34-digit context, and the
//! conversion to and from Arrow per item 47's spec-type mapping.

use std::cmp::Ordering;
use std::fmt;
use std::sync::Arc;

use arrow::array::{
    Array, ArrayRef, BooleanArray, Date32Array, Decimal128Array, Float64Array, StringArray,
    TimestampNanosecondArray,
};
use arrow::datatypes::{DataType, TimeUnit};
use bigdecimal::num_bigint::{BigInt, Sign};
use bigdecimal::{BigDecimal, RoundingMode};
use serde_json::{Map, Value, json};
use unicode_normalization::is_nfc;

use crate::errors::{Code, Error, ptr};

/// Significant digits of the decimal context (11.6, 11.18.2).
pub const DECIMAL_DIGITS: u64 = 34;
/// Significant digits kept from an IEEE double result (11.6).
pub const DOUBLE_DIGITS: usize = 15;

/// A calendar date, proleptic Gregorian, years 1 to 9999.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date {
    /// Year, 1 to 9999.
    pub year: i32,
    /// Month, 1 to 12.
    pub month: u32,
    /// Day of month.
    pub day: u32,
}

/// A UTC instant with nanosecond precision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DateTime {
    /// The date.
    pub date: Date,
    /// Hour, 0 to 23.
    pub hour: u32,
    /// Minute, 0 to 59.
    pub minute: u32,
    /// Second, 0 to 59.
    pub second: u32,
    /// Nanoseconds, 0 to `999_999_999`.
    pub nanos: u32,
}

/// One typed value. Equality is equality of encodings: `1.20` and `1.2`
/// are different `dec` values.
#[derive(Debug, Clone)]
pub enum TypedValue {
    /// `{"t":"int","v":"42"}`.
    Int(BigInt),
    /// `{"t":"dec","v":"0.00310"}`, scale as written (never negative).
    Dec(BigDecimal),
    /// `{"t":"str","v":"..."}`, NFC.
    Str(String),
    /// `{"t":"bool","v":true}`.
    Bool(bool),
    /// `{"t":"date","v":"2026-11-09"}`.
    Date(Date),
    /// `{"t":"datetime","v":"2026-11-09T14:02:11.5Z"}`.
    DateTime(DateTime),
    /// `{"t":"list","v":[...]}`, settings only; never nested.
    List(Vec<TypedValue>),
    /// `{"t":"missing"}`.
    Missing,
    /// `{"t":"nan"}`.
    Nan,
    /// `{"t":"inf","s":"+"}` (`true`) or `"-"` (`false`).
    Inf(bool),
}

impl PartialEq for TypedValue {
    #[allow(clippy::match_same_arms, reason = "the arms bind different types")]
    fn eq(&self, other: &TypedValue) -> bool {
        match (self, other) {
            (TypedValue::Dec(a), TypedValue::Dec(b)) => {
                a.as_bigint_and_exponent() == b.as_bigint_and_exponent()
            }
            (TypedValue::Int(a), TypedValue::Int(b)) => a == b,
            (TypedValue::Str(a), TypedValue::Str(b)) => a == b,
            (TypedValue::Bool(a), TypedValue::Bool(b)) => a == b,
            (TypedValue::Date(a), TypedValue::Date(b)) => a == b,
            (TypedValue::DateTime(a), TypedValue::DateTime(b)) => a == b,
            (TypedValue::List(a), TypedValue::List(b)) => a == b,
            (TypedValue::Missing, TypedValue::Missing) | (TypedValue::Nan, TypedValue::Nan) => true,
            (TypedValue::Inf(a), TypedValue::Inf(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for TypedValue {}

/// The type tag of a typed value, as used by type checking (11.18.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tag {
    /// `int`.
    Int,
    /// `dec`.
    Dec,
    /// `str`.
    Str,
    /// `bool`.
    Bool,
    /// `date`.
    Date,
    /// `datetime`.
    DateTime,
    /// `list`.
    List,
    /// `missing`.
    Missing,
    /// `nan`.
    Nan,
    /// `inf`.
    Inf,
}

impl Tag {
    /// The 11.6 `t` spelling.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Tag::Int => "int",
            Tag::Dec => "dec",
            Tag::Str => "str",
            Tag::Bool => "bool",
            Tag::Date => "date",
            Tag::DateTime => "datetime",
            Tag::List => "list",
            Tag::Missing => "missing",
            Tag::Nan => "nan",
            Tag::Inf => "inf",
        }
    }
}

/// A parse-spec column type (11.7) and item 47's Arrow mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SpecType {
    /// `int`: `Decimal128(38, 0)`.
    Int,
    /// `dec`: `Decimal128(38, s)`.
    Dec,
    /// `str`: `Utf8`.
    Str,
    /// `bool`: `Boolean`.
    Bool,
    /// `date`: `Date32`.
    Date,
    /// `datetime`: `Timestamp(Nanosecond, UTC)`.
    DateTime,
    /// `float64`: `Float64`; its values are `dec` (11.6).
    Float64,
}

impl SpecType {
    /// Parse the 11.7 spelling.
    #[must_use]
    pub fn parse(s: &str) -> Option<SpecType> {
        Some(match s {
            "int" => SpecType::Int,
            "dec" => SpecType::Dec,
            "str" => SpecType::Str,
            "bool" => SpecType::Bool,
            "date" => SpecType::Date,
            "datetime" => SpecType::DateTime,
            "float64" => SpecType::Float64,
            _ => return None,
        })
    }

    /// The 11.7 spelling.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            SpecType::Int => "int",
            SpecType::Dec => "dec",
            SpecType::Str => "str",
            SpecType::Bool => "bool",
            SpecType::Date => "date",
            SpecType::DateTime => "datetime",
            SpecType::Float64 => "float64",
        }
    }

    /// The typed-value tag of this column's values (`float64` is `dec`).
    #[must_use]
    pub fn tag(self) -> Tag {
        match self {
            SpecType::Int => Tag::Int,
            SpecType::Dec | SpecType::Float64 => Tag::Dec,
            SpecType::Str => Tag::Str,
            SpecType::Bool => Tag::Bool,
            SpecType::Date => Tag::Date,
            SpecType::DateTime => Tag::DateTime,
        }
    }
}

fn e1004(at: &str, msg: impl Into<String>) -> Error {
    Error::at(Code::E1004, at, msg)
}

// ---------------------------------------------------------------- numbers

/// Parse an `int` `v`: `-?(0|[1-9][0-9]*)`, no minus zero.
#[must_use]
pub fn parse_int_text(s: &str) -> Option<BigInt> {
    let (neg, digits) = match s.strip_prefix('-') {
        Some(d) => (true, d),
        None => (false, s),
    };
    if !plain_digits(digits) || (neg && digits == "0") {
        return None;
    }
    digits
        .parse::<BigInt>()
        .ok()
        .map(|n| if neg { -n } else { n })
}

fn plain_digits(d: &str) -> bool {
    !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()) && (d == "0" || !d.starts_with('0'))
}

/// Parse a `dec` `v`: `-?(0|[1-9][0-9]*)(\.[0-9]+)?`, no minus on zero;
/// the scale is the number of fraction digits as written.
#[must_use]
pub fn parse_dec_text(s: &str) -> Option<BigDecimal> {
    let (neg, body) = match s.strip_prefix('-') {
        Some(d) => (true, d),
        None => (false, s),
    };
    let (int, frac) = match body.split_once('.') {
        Some((i, f)) => {
            if f.is_empty() || !f.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            (i, f)
        }
        None => (body, ""),
    };
    if !plain_digits(int) {
        return None;
    }
    let all: String = [int, frac].concat();
    if neg && all.bytes().all(|b| b == b'0') {
        return None;
    }
    let mut n: BigInt = all.parse().ok()?;
    if neg {
        n = -n;
    }
    Some(BigDecimal::new(n, i64::try_from(frac.len()).ok()?))
}

/// The positional text of a decimal: no exponent, scale kept, no minus on
/// zero. A negative scale is written as trailing zeros.
#[must_use]
pub fn dec_text(d: &BigDecimal) -> String {
    let (n, scale) = d.as_bigint_and_exponent();
    let neg = n.sign() == Sign::Minus;
    let mut digits = n.magnitude().to_string();
    let mut out = String::new();
    if neg {
        out.push('-');
    }
    if scale <= 0 {
        if digits != "0" {
            for _ in 0..(-scale) {
                digits.push('0');
            }
        }
        out.push_str(&digits);
    } else {
        let scale = usize::try_from(scale).unwrap_or(0);
        if digits.len() <= scale {
            let pad = scale + 1 - digits.len();
            digits.insert_str(0, &"0".repeat(pad));
        }
        let point = digits.len() - scale;
        out.push_str(&digits[..point]);
        out.push('.');
        out.push_str(&digits[point..]);
    }
    out
}

/// Round to the 34-digit context, half-even (11.6, 11.18.2). Values with
/// 34 or fewer significant digits are returned unchanged, scale included.
#[must_use]
pub fn round_to_context(d: &BigDecimal) -> BigDecimal {
    let digits = d.digits();
    if digits <= DECIMAL_DIGITS {
        return d.clone();
    }
    let (_, scale) = d.as_bigint_and_exponent();
    let drop = i64::try_from(digits - DECIMAL_DIGITS).unwrap_or(i64::MAX);
    d.with_scale_round(scale - drop, RoundingMode::HalfEven)
}

/// Build a positional decimal from a digit string and a decimal exponent
/// (`value = 0.d1d2d3... * 10^point`-style input is avoided: here
/// `value = digits * 10^exp`). Trailing zeros of `digits` are kept.
fn dec_from_digits(neg: bool, digits: &str, exp: i64) -> BigDecimal {
    let mut n: BigInt = digits.parse().unwrap_or_default();
    if neg {
        n = -n;
    }
    if exp >= 0 {
        // Integer: expand to scale 0 so the text has no exponent.
        let mut s = digits.to_owned();
        if s != "0" {
            for _ in 0..exp {
                s.push('0');
            }
        }
        let mut m: BigInt = s.parse().unwrap_or_default();
        if neg {
            m = -m;
        }
        BigDecimal::new(m, 0)
    } else {
        BigDecimal::new(n, -exp)
    }
}

/// Split Rust's `{:e}` output (`d.ddde±x`) into sign, digits without the
/// point, and the exponent of the last digit.
fn split_exp(s: &str) -> (bool, String, i64) {
    let (neg, s) = match s.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, s),
    };
    let (mant, exp) = s.split_once('e').unwrap_or((s, "0"));
    let exp: i64 = exp.parse().unwrap_or(0);
    let (int, frac) = mant.split_once('.').unwrap_or((mant, ""));
    let digits = format!("{int}{frac}");
    let frac_len = i64::try_from(frac.len()).unwrap_or(0);
    (neg, digits, exp - frac_len)
}

fn strip_trailing_zeros(digits: &mut String, exp: &mut i64) {
    while digits.len() > 1 && digits.ends_with('0') {
        digits.pop();
        *exp += 1;
    }
}

/// An IEEE double result (`log`, `exp`, `mean`, `median`) as a `dec`:
/// rounded to 15 significant digits, then the shortest digits at that
/// precision, positional (11.6). `None` for NaN and infinities.
#[must_use]
pub fn dec_from_f64_round15(x: f64) -> Option<BigDecimal> {
    if !x.is_finite() {
        return None;
    }
    if x == 0.0 {
        return Some(BigDecimal::new(BigInt::from(0), 0));
    }
    let s = format!("{:.*e}", DOUBLE_DIGITS - 1, x);
    let (neg, mut digits, mut exp) = split_exp(&s);
    strip_trailing_zeros(&mut digits, &mut exp);
    Some(dec_from_digits(neg, &digits, exp))
}

/// A `float64` column value as a `dec`: the shortest round-trip digits of
/// the IEEE value, positional, either zero written `0` (11.6). `None` for
/// NaN and infinities.
#[must_use]
pub fn dec_from_f64_shortest(x: f64) -> Option<BigDecimal> {
    if !x.is_finite() {
        return None;
    }
    if x == 0.0 {
        return Some(BigDecimal::new(BigInt::from(0), 0));
    }
    let s = format!("{x:e}");
    let (neg, mut digits, mut exp) = split_exp(&s);
    strip_trailing_zeros(&mut digits, &mut exp);
    Some(dec_from_digits(neg, &digits, exp))
}

// ---------------------------------------------------------------- dates

fn days_in_month(y: i32, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 => 29,
        _ => 28,
    }
}

fn fixed_digits(s: &str) -> Option<u32> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

impl Date {
    /// Parse `YYYY-MM-DD`.
    #[must_use]
    pub fn parse(s: &str) -> Option<Date> {
        let b = s.as_bytes();
        if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
            return None;
        }
        let year = i32::try_from(fixed_digits(&s[0..4])?).ok()?;
        let month = fixed_digits(&s[5..7])?;
        let day = fixed_digits(&s[8..10])?;
        if year < 1 || !(1..=12).contains(&month) || day < 1 || day > days_in_month(year, month) {
            return None;
        }
        Some(Date { year, month, day })
    }

    /// Days since 1970-01-01 (Howard Hinnant's `days_from_civil`).
    #[must_use]
    pub fn days_from_epoch(self) -> i64 {
        let y = i64::from(self.year) - i64::from(self.month <= 2);
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let m = i64::from(self.month);
        let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(self.day) - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146_097 + doe - 719_468
    }

    /// The date `days` after 1970-01-01, if it is in years 1 to 9999.
    #[must_use]
    pub fn from_days(days: i64) -> Option<Date> {
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = u32::try_from(doy - (153 * mp + 2) / 5 + 1).ok()?;
        let month = u32::try_from(if mp < 10 { mp + 3 } else { mp - 9 }).ok()?;
        let year = i32::try_from(yoe + era * 400 + i64::from(month <= 2)).ok()?;
        (1..=9999)
            .contains(&year)
            .then_some(Date { year, month, day })
    }
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

impl DateTime {
    /// Parse `YYYY-MM-DDTHH:MM:SS[.f{1,9}]Z`, fraction without trailing zero.
    #[must_use]
    pub fn parse(s: &str) -> Option<DateTime> {
        let body = s.strip_suffix('Z')?;
        if body.len() < 19 || body.as_bytes()[10] != b'T' {
            return None;
        }
        let date = Date::parse(&body[..10])?;
        let t = &body[11..];
        let (hms, frac) = match t.split_once('.') {
            Some((h, f)) => (h, Some(f)),
            None => (t, None),
        };
        let hb = hms.as_bytes();
        if hb.len() != 8 || hb[2] != b':' || hb[5] != b':' {
            return None;
        }
        let hour = fixed_digits(&hms[0..2])?;
        let minute = fixed_digits(&hms[3..5])?;
        let second = fixed_digits(&hms[6..8])?;
        if hour > 23 || minute > 59 || second > 59 {
            return None;
        }
        let nanos = match frac {
            None => 0,
            Some(f) => {
                if f.is_empty() || f.len() > 9 || f.ends_with('0') {
                    return None;
                }
                let v = fixed_digits(f)?;
                v * 10u32.pow(9 - u32::try_from(f.len()).ok()?)
            }
        };
        Some(DateTime {
            date,
            hour,
            minute,
            second,
            nanos,
        })
    }

    /// Nanoseconds since the Unix epoch, if it fits an `i64`.
    #[must_use]
    pub fn epoch_nanos(self) -> Option<i64> {
        let secs = self.date.days_from_epoch() * 86_400
            + i64::from(self.hour) * 3600
            + i64::from(self.minute) * 60
            + i64::from(self.second);
        secs.checked_mul(1_000_000_000)?
            .checked_add(i64::from(self.nanos))
    }

    /// The instant `nanos` after the Unix epoch.
    #[must_use]
    pub fn from_epoch_nanos(nanos: i64) -> Option<DateTime> {
        let secs = nanos.div_euclid(1_000_000_000);
        let sub = u32::try_from(nanos.rem_euclid(1_000_000_000)).ok()?;
        let days = secs.div_euclid(86_400);
        let rem = u32::try_from(secs.rem_euclid(86_400)).ok()?;
        Some(DateTime {
            date: Date::from_days(days)?,
            hour: rem / 3600,
            minute: rem % 3600 / 60,
            second: rem % 60,
            nanos: sub,
        })
    }
}

impl fmt::Display for DateTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}T{:02}:{:02}:{:02}",
            self.date, self.hour, self.minute, self.second
        )?;
        if self.nanos != 0 {
            let s = format!("{:09}", self.nanos);
            write!(f, ".{}", s.trim_end_matches('0'))?;
        }
        f.write_str("Z")
    }
}

// ---------------------------------------------------------------- JSON

impl TypedValue {
    /// The type tag.
    #[must_use]
    pub fn tag(&self) -> Tag {
        match self {
            TypedValue::Int(_) => Tag::Int,
            TypedValue::Dec(_) => Tag::Dec,
            TypedValue::Str(_) => Tag::Str,
            TypedValue::Bool(_) => Tag::Bool,
            TypedValue::Date(_) => Tag::Date,
            TypedValue::DateTime(_) => Tag::DateTime,
            TypedValue::List(_) => Tag::List,
            TypedValue::Missing => Tag::Missing,
            TypedValue::Nan => Tag::Nan,
            TypedValue::Inf(_) => Tag::Inf,
        }
    }

    /// Read a typed value, checking its lexical form (11.6). Errors are
    /// `E1004` with `field` under `at`.
    ///
    /// # Errors
    /// `E1004` for any deviation from 11.6.
    pub fn from_json(v: &Value, at: &str) -> Result<TypedValue, Error> {
        Self::read(v, at, true)
    }

    #[allow(clippy::too_many_lines, reason = "one arm per 11.6 type")]
    fn read(v: &Value, at: &str, allow_list: bool) -> Result<TypedValue, Error> {
        let Some(m) = v.as_object() else {
            return Err(e1004(at, "a typed value is an object"));
        };
        let Some(t) = m.get("t") else {
            return Err(e1004(&ptr(at, "t"), "a typed value has `t`"));
        };
        let Some(t) = t.as_str() else {
            return Err(e1004(&ptr(at, "t"), "`t` is a string"));
        };
        let expect = |keys: &[&str]| -> Result<(), Error> {
            for k in keys {
                if !m.contains_key(*k) {
                    return Err(e1004(&ptr(at, k), format!("`{t}` has `{k}`")));
                }
            }
            let mut extra: Vec<&String> =
                m.keys().filter(|k| !keys.contains(&k.as_str())).collect();
            extra.sort_by(|a, b| crate::canonical::utf16_cmp(a, b));
            if let Some(k) = extra.first() {
                return Err(e1004(&ptr(at, k), format!("`{t}` has no member `{k}`")));
            }
            Ok(())
        };
        let vat = ptr(at, "v");
        let vstr = |m: &Map<String, Value>| -> Result<String, Error> {
            m.get("v")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .ok_or_else(|| e1004(&vat, format!("`v` of `{t}` is a string")))
        };
        Ok(match t {
            "int" => {
                expect(&["t", "v"])?;
                let s = vstr(m)?;
                TypedValue::Int(
                    parse_int_text(&s).ok_or_else(|| e1004(&vat, format!("not an int: {s:?}")))?,
                )
            }
            "dec" => {
                expect(&["t", "v"])?;
                let s = vstr(m)?;
                TypedValue::Dec(
                    parse_dec_text(&s).ok_or_else(|| e1004(&vat, format!("not a dec: {s:?}")))?,
                )
            }
            "str" => {
                expect(&["t", "v"])?;
                let s = vstr(m)?;
                if !is_nfc(&s) {
                    return Err(e1004(&vat, "a str is NFC"));
                }
                TypedValue::Str(s)
            }
            "bool" => {
                expect(&["t", "v"])?;
                TypedValue::Bool(
                    m["v"]
                        .as_bool()
                        .ok_or_else(|| e1004(&vat, "`v` of `bool` is a boolean"))?,
                )
            }
            "date" => {
                expect(&["t", "v"])?;
                let s = vstr(m)?;
                TypedValue::Date(
                    Date::parse(&s).ok_or_else(|| e1004(&vat, format!("not a date: {s:?}")))?,
                )
            }
            "datetime" => {
                expect(&["t", "v"])?;
                let s = vstr(m)?;
                TypedValue::DateTime(
                    DateTime::parse(&s)
                        .ok_or_else(|| e1004(&vat, format!("not a datetime: {s:?}")))?,
                )
            }
            "list" => {
                expect(&["t", "v"])?;
                if !allow_list {
                    return Err(e1004(&ptr(at, "t"), "a list does not nest"));
                }
                let Some(items) = m["v"].as_array() else {
                    return Err(e1004(&vat, "`v` of `list` is a list"));
                };
                let mut out = Vec::with_capacity(items.len());
                for (i, item) in items.iter().enumerate() {
                    out.push(Self::read(item, &ptr(&vat, i), false)?);
                }
                TypedValue::List(out)
            }
            "missing" => {
                expect(&["t"])?;
                TypedValue::Missing
            }
            "nan" => {
                expect(&["t"])?;
                TypedValue::Nan
            }
            "inf" => {
                expect(&["t", "s"])?;
                match m["s"].as_str() {
                    Some("+") => TypedValue::Inf(true),
                    Some("-") => TypedValue::Inf(false),
                    _ => return Err(e1004(&ptr(at, "s"), "`s` is \"+\" or \"-\"")),
                }
            }
            other => return Err(e1004(&ptr(at, "t"), format!("unknown type {other:?}"))),
        })
    }

    /// The JSON encoding.
    #[must_use]
    pub fn to_json(&self) -> Value {
        match self {
            TypedValue::Int(n) => json!({"t": "int", "v": n.to_string()}),
            TypedValue::Dec(d) => json!({"t": "dec", "v": dec_text(d)}),
            TypedValue::Str(s) => json!({"t": "str", "v": s}),
            TypedValue::Bool(b) => json!({"t": "bool", "v": b}),
            TypedValue::Date(d) => json!({"t": "date", "v": d.to_string()}),
            TypedValue::DateTime(d) => json!({"t": "datetime", "v": d.to_string()}),
            TypedValue::List(items) => {
                json!({"t": "list", "v": items.iter().map(TypedValue::to_json).collect::<Vec<_>>()})
            }
            TypedValue::Missing => json!({"t": "missing"}),
            TypedValue::Nan => json!({"t": "nan"}),
            TypedValue::Inf(pos) => json!({"t": "inf", "s": if *pos { "+" } else { "-" }}),
        }
    }

    /// A `str` value, NFC-normalized from any input.
    #[must_use]
    pub fn str_nfc(s: &str) -> TypedValue {
        TypedValue::Str(crate::sort_order::nfc(s))
    }

    /// The exact decimal value of an `int` or `dec`.
    #[must_use]
    pub fn as_decimal(&self) -> Option<BigDecimal> {
        match self {
            TypedValue::Int(n) => Some(BigDecimal::new(n.clone(), 0)),
            TypedValue::Dec(d) => Some(d.clone()),
            _ => None,
        }
    }

    /// Read a value from its `v` text for a column type, as the CLI
    /// citation spelling does (11.23). `None` if it does not parse.
    #[must_use]
    pub fn from_text(text: &str, t: SpecType) -> Option<TypedValue> {
        Some(match t {
            SpecType::Int => TypedValue::Int(parse_int_text(text)?),
            SpecType::Dec | SpecType::Float64 => TypedValue::Dec(parse_dec_text(text)?),
            SpecType::Str => TypedValue::str_nfc(text),
            SpecType::Bool => match text {
                "true" => TypedValue::Bool(true),
                "false" => TypedValue::Bool(false),
                _ => return None,
            },
            SpecType::Date => TypedValue::Date(Date::parse(text)?),
            SpecType::DateTime => TypedValue::DateTime(DateTime::parse(text)?),
        })
    }

    /// Compare two values of comparable types: numbers exactly as decimals,
    /// strings in 11.10 order, booleans, and dates or datetimes
    /// chronologically. `None` for any other pair (a date with a datetime
    /// included) and for `missing`, `nan`, `inf`, and `list`.
    #[must_use]
    pub fn compare(&self, other: &TypedValue) -> Option<Ordering> {
        match (self, other) {
            (TypedValue::Str(a), TypedValue::Str(b)) => Some(crate::sort_order::cmp_text(a, b)),
            (TypedValue::Bool(a), TypedValue::Bool(b)) => Some(a.cmp(b)),
            (TypedValue::Date(a), TypedValue::Date(b)) => Some(a.cmp(b)),
            (TypedValue::DateTime(a), TypedValue::DateTime(b)) => Some(a.cmp(b)),
            _ => Some(self.as_decimal()?.cmp(&other.as_decimal()?)),
        }
    }
}

// ---------------------------------------------------------------- Arrow

/// Item 47's Arrow type for a spec column type; `scale` is the `dec`
/// column's scale (11.7).
#[must_use]
pub fn arrow_type(t: SpecType, scale: u8) -> DataType {
    match t {
        SpecType::Int => DataType::Decimal128(38, 0),
        SpecType::Dec => DataType::Decimal128(38, i8::try_from(scale).unwrap_or(38)),
        SpecType::Str => DataType::Utf8,
        SpecType::Bool => DataType::Boolean,
        SpecType::Date => DataType::Date32,
        SpecType::DateTime => DataType::Timestamp(TimeUnit::Nanosecond, Some("UTC".into())),
        SpecType::Float64 => DataType::Float64,
    }
}

fn e3102(msg: impl Into<String>) -> Error {
    Error::new(Code::E3102, msg)
}

const MAX_DEC128: i128 = 99_999_999_999_999_999_999_999_999_999_999_999_999;

fn to_i128_at_scale(d: &BigDecimal, scale: u8) -> Result<i128, Error> {
    let (_, s) = d.as_bigint_and_exponent();
    if s > i64::from(scale) {
        return Err(e3102(format!(
            "{} does not fit at scale {scale}",
            dec_text(d)
        )));
    }
    let (n, _) = d.with_scale(i64::from(scale)).into_bigint_and_exponent();
    let v: i128 = i128::try_from(n).map_err(|_| e3102("value wider than 38 digits"))?;
    if v.abs() > MAX_DEC128 {
        return Err(e3102("value wider than 38 digits"));
    }
    Ok(v)
}

fn mismatch(t: SpecType, v: &TypedValue) -> Error {
    e3102(format!("a {:?} value in a {} column", v.tag(), t.as_str()))
}

/// Convert a column of typed values to an Arrow array of item 47's type.
/// `missing` becomes null; `nan` and `inf` are accepted in `float64`
/// columns only.
///
/// # Errors
/// `E3102` for a value of another type, or one that does not fit.
pub fn to_arrow(values: &[TypedValue], t: SpecType, scale: u8) -> Result<ArrayRef, Error> {
    let null = |v: &TypedValue| matches!(v, TypedValue::Missing);
    Ok(match t {
        SpecType::Int | SpecType::Dec => {
            let s = if t == SpecType::Int { 0 } else { scale };
            let mut out = Vec::with_capacity(values.len());
            for v in values {
                out.push(match v {
                    TypedValue::Missing => None,
                    TypedValue::Int(_) if t == SpecType::Int => {
                        Some(to_i128_at_scale(&v.as_decimal().unwrap_or_default(), 0)?)
                    }
                    TypedValue::Dec(d) if t == SpecType::Dec => Some(to_i128_at_scale(d, s)?),
                    other => return Err(mismatch(t, other)),
                });
            }
            let precision_scale = i8::try_from(s).map_err(|_| e3102("scale above 38"))?;
            Arc::new(
                Decimal128Array::from(out)
                    .with_precision_and_scale(38, precision_scale)
                    .map_err(|e| e3102(e.to_string()))?,
            )
        }
        SpecType::Str => {
            let mut out: Vec<Option<String>> = Vec::with_capacity(values.len());
            for v in values {
                out.push(match v {
                    TypedValue::Str(s) => Some(crate::sort_order::nfc(s)),
                    v if null(v) => None,
                    other => return Err(mismatch(t, other)),
                });
            }
            Arc::new(StringArray::from(out))
        }
        SpecType::Bool => {
            let mut out = Vec::with_capacity(values.len());
            for v in values {
                out.push(match v {
                    TypedValue::Bool(b) => Some(*b),
                    v if null(v) => None,
                    other => return Err(mismatch(t, other)),
                });
            }
            Arc::new(BooleanArray::from(out))
        }
        SpecType::Date => {
            let mut out = Vec::with_capacity(values.len());
            for v in values {
                out.push(match v {
                    TypedValue::Date(d) => Some(
                        i32::try_from(d.days_from_epoch())
                            .map_err(|_| e3102("date out of range"))?,
                    ),
                    v if null(v) => None,
                    other => return Err(mismatch(t, other)),
                });
            }
            Arc::new(Date32Array::from(out))
        }
        SpecType::DateTime => {
            let mut out = Vec::with_capacity(values.len());
            for v in values {
                out.push(match v {
                    TypedValue::DateTime(d) => Some(d.epoch_nanos().ok_or_else(|| {
                        e3102(format!("{d} is outside the nanosecond timestamp range"))
                    })?),
                    v if null(v) => None,
                    other => return Err(mismatch(t, other)),
                });
            }
            Arc::new(TimestampNanosecondArray::from(out).with_timezone("UTC"))
        }
        SpecType::Float64 => {
            let mut out = Vec::with_capacity(values.len());
            for v in values {
                out.push(match v {
                    TypedValue::Dec(d) => Some(
                        dec_text(d)
                            .parse::<f64>()
                            .map_err(|e| e3102(e.to_string()))?,
                    ),
                    TypedValue::Nan => Some(f64::NAN),
                    TypedValue::Inf(true) => Some(f64::INFINITY),
                    TypedValue::Inf(false) => Some(f64::NEG_INFINITY),
                    v if null(v) => None,
                    other => return Err(mismatch(t, other)),
                });
            }
            Arc::new(Float64Array::from(out))
        }
    })
}

fn downcast<T: 'static>(array: &dyn Array, t: SpecType) -> Result<&T, Error> {
    array.as_any().downcast_ref::<T>().ok_or_else(|| {
        e3102(format!(
            "array of type {} is not a {} column",
            array.data_type(),
            t.as_str()
        ))
    })
}

/// Read row `row` of an Arrow array holding a column of type `t`. Null is
/// `missing`; a `float64` NaN or infinity is `nan` or `inf`.
///
/// # Errors
/// `E3102` if the array is not of item 47's type for `t` or `row` is out
/// of range.
pub fn from_arrow(array: &dyn Array, row: usize, t: SpecType) -> Result<TypedValue, Error> {
    if row >= array.len() {
        return Err(e3102(format!("row {row} out of range")));
    }
    if array.is_null(row) {
        return Ok(TypedValue::Missing);
    }
    Ok(match t {
        SpecType::Int | SpecType::Dec => {
            let a = downcast::<Decimal128Array>(array, t)?;
            let scale = i64::from(a.scale());
            let d = BigDecimal::new(BigInt::from(a.value(row)), scale);
            if t == SpecType::Int {
                if scale != 0 {
                    return Err(e3102("an int column has scale 0"));
                }
                TypedValue::Int(BigInt::from(a.value(row)))
            } else {
                TypedValue::Dec(d)
            }
        }
        SpecType::Str => TypedValue::str_nfc(downcast::<StringArray>(array, t)?.value(row)),
        SpecType::Bool => TypedValue::Bool(downcast::<BooleanArray>(array, t)?.value(row)),
        SpecType::Date => {
            let days = downcast::<Date32Array>(array, t)?.value(row);
            TypedValue::Date(
                Date::from_days(i64::from(days))
                    .ok_or_else(|| e3102("date outside years 1 to 9999"))?,
            )
        }
        SpecType::DateTime => {
            let ns = downcast::<TimestampNanosecondArray>(array, t)?.value(row);
            TypedValue::DateTime(
                DateTime::from_epoch_nanos(ns).ok_or_else(|| e3102("timestamp out of range"))?,
            )
        }
        SpecType::Float64 => {
            let x = downcast::<Float64Array>(array, t)?.value(row);
            if x.is_nan() {
                TypedValue::Nan
            } else if x.is_infinite() {
                TypedValue::Inf(x > 0.0)
            } else {
                TypedValue::Dec(dec_from_f64_shortest(x).unwrap_or_default())
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dec(s: &str) -> BigDecimal {
        parse_dec_text(s).unwrap()
    }

    #[test]
    fn dec_text_keeps_scale() {
        for s in ["0.00310", "0", "12", "-1.50", "0.0"] {
            assert_eq!(dec_text(&dec(s)), s);
        }
        assert!(parse_dec_text("-0.00").is_none());
        assert!(parse_dec_text("1e5").is_none());
        assert!(parse_dec_text(".5").is_none());
        assert!(parse_dec_text("01.5").is_none());
        // Negative zero from arithmetic is written without its sign.
        let z = dec("-1.50") * BigDecimal::from(0);
        assert_eq!(dec_text(&z), "0.00");
    }

    #[test]
    fn context_rounds_half_even() {
        let d = dec("1.0000000000000000000000000000000005"); // 35 digits
        assert_eq!(
            dec_text(&round_to_context(&d)),
            "1.000000000000000000000000000000000"
        );
        let d = dec("1.0000000000000000000000000000000015");
        assert_eq!(
            dec_text(&round_to_context(&d)),
            "1.000000000000000000000000000000002"
        );
    }

    #[test]
    fn doubles() {
        assert_eq!(dec_text(&dec_from_f64_round15(0.1).unwrap()), "0.1");
        assert_eq!(
            dec_text(&dec_from_f64_round15(1.0 / 3.0).unwrap()),
            "0.333333333333333"
        );
        assert_eq!(dec_text(&dec_from_f64_round15(1e-7).unwrap()), "0.0000001");
        assert_eq!(
            dec_text(&dec_from_f64_round15(1.234_567_890_123_456_7e17).unwrap()),
            "123456789012346000"
        );
        assert_eq!(dec_text(&dec_from_f64_round15(-0.0).unwrap()), "0");
        assert_eq!(dec_text(&dec_from_f64_shortest(1e-5).unwrap()), "0.00001");
        assert_eq!(
            dec_text(&dec_from_f64_shortest(0.1 + 0.2).unwrap()),
            "0.30000000000000004"
        );
        assert_eq!(
            dec_text(&dec_from_f64_shortest(1e21).unwrap()),
            "1000000000000000000000"
        );
        assert!(dec_from_f64_shortest(f64::NAN).is_none());
    }

    #[test]
    fn datetimes() {
        let d = DateTime::parse("2026-11-09T14:02:11.5Z").unwrap();
        assert_eq!(d.nanos, 500_000_000);
        assert_eq!(d.to_string(), "2026-11-09T14:02:11.5Z");
        assert_eq!(
            DateTime::from_epoch_nanos(d.epoch_nanos().unwrap()).unwrap(),
            d
        );
        assert!(DateTime::parse("2026-11-09T14:02:11.50Z").is_none());
        assert!(DateTime::parse("2026-11-09T14:02:11+00:00").is_none());
        assert!(Date::parse("2026-02-29").is_none());
        assert!(Date::parse("2024-02-29").is_some());
        assert_eq!(Date::parse("1970-01-01").unwrap().days_from_epoch(), 0);
        assert_eq!(Date::from_days(-719_162).unwrap().to_string(), "0001-01-01");
    }

    #[test]
    fn arrow_round_trip() {
        let cases: Vec<(SpecType, u8, Vec<TypedValue>)> = vec![
            (
                SpecType::Int,
                0,
                vec![TypedValue::Int(BigInt::from(-42)), TypedValue::Missing],
            ),
            (
                SpecType::Dec,
                3,
                vec![
                    TypedValue::Dec(dec("1.250")),
                    TypedValue::Dec(dec("-0.001")),
                ],
            ),
            (
                SpecType::Str,
                0,
                vec![TypedValue::Str("TP53".into()), TypedValue::Missing],
            ),
            (SpecType::Bool, 0, vec![TypedValue::Bool(true)]),
            (
                SpecType::Date,
                0,
                vec![TypedValue::Date(Date::parse("2026-11-09").unwrap())],
            ),
            (
                SpecType::DateTime,
                0,
                vec![TypedValue::DateTime(
                    DateTime::parse("2026-11-09T14:02:11.000000001Z").unwrap(),
                )],
            ),
            (
                SpecType::Float64,
                0,
                vec![
                    TypedValue::Dec(dec("0.1")),
                    TypedValue::Nan,
                    TypedValue::Inf(false),
                ],
            ),
        ];
        for (t, scale, values) in cases {
            let a = to_arrow(&values, t, scale).unwrap();
            assert_eq!(a.data_type(), &arrow_type(t, scale));
            for (i, v) in values.iter().enumerate() {
                assert_eq!(&from_arrow(a.as_ref(), i, t).unwrap(), v, "{t:?} row {i}");
            }
        }
        // Scale above the column's is a parse failure; ints keep scale 0.
        assert_eq!(
            to_arrow(&[TypedValue::Dec(dec("1.2345"))], SpecType::Dec, 3)
                .unwrap_err()
                .code,
            Code::E3102
        );
        let a = to_arrow(&[TypedValue::Dec(dec("1.2"))], SpecType::Dec, 3).unwrap();
        assert_eq!(
            from_arrow(a.as_ref(), 0, SpecType::Dec).unwrap(),
            TypedValue::Dec(dec("1.200"))
        );
    }
}
