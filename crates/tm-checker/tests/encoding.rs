//! The property the Checker's correctness rests on (11.18.2 Where, item
//! 53): two values equal under the `=` rule encode to identical keys, and
//! the keys of two unequal values order as 11.18.2 orders the values.
//!
//! The oracle, `value_order`, is written here from 11.18.2's rules and does
//! not use the encoder: numbers by exact value, strings by NFC bytes (11.10),
//! dates and datetimes chronologically, `false` before `true`. Text keys are
//! compared by their bytes in Rust and, with `bool` (which enters `DuckDB`
//! as `BOOLEAN`, not as a text key), by `DuckDB`'s own `ORDER BY` under the
//! collation the translator writes.

mod common;

use std::cmp::Ordering;

use bigdecimal::BigDecimal;
use bigdecimal::num_bigint::BigInt;
use proptest::prelude::*;

use tm_checker::engine::{CheckEngine, rows};
use tm_checker::table::{Table, encode, order_key};
use tm_checker::translate::COLLATE;
use tm_formats::typed_value::{Date, DateTime, Tag, TypedValue, parse_dec_text};

use common::case;

fn dec(s: &str) -> TypedValue {
    TypedValue::Dec(parse_dec_text(s).unwrap_or_else(|| panic!("dec {s}")))
}

/// `-0.00` as arithmetic leaves it (the typed-value text form refuses a
/// minus on zero, so this is the only way it reaches the encoder).
fn neg_zero() -> TypedValue {
    TypedValue::Dec(
        "-0.00"
            .parse::<BigDecimal>()
            .unwrap_or_else(|e| panic!("{e}")),
    )
}

fn int(n: i64) -> TypedValue {
    TypedValue::Int(BigInt::from(n))
}

fn date(year: i32, month: u32, day: u32) -> Date {
    Date { year, month, day }
}

fn datetime(d: Date, hour: u32, minute: u32, second: u32, nanos: u32) -> TypedValue {
    TypedValue::DateTime(DateTime {
        date: d,
        hour,
        minute,
        second,
        nanos,
    })
}

fn key(v: &TypedValue) -> String {
    order_key(v).unwrap_or_else(|| panic!("no text key for {v:?}"))
}

/// 11.18.2 order of two values of comparable types, from the rules alone.
fn value_order(a: &TypedValue, b: &TypedValue) -> Ordering {
    let num = |v: &TypedValue| match v {
        TypedValue::Int(n) => Some(BigDecimal::new(n.clone(), 0)),
        TypedValue::Dec(d) => Some(d.clone()),
        _ => None,
    };
    match (a, b) {
        (TypedValue::Str(x), TypedValue::Str(y)) => x.as_bytes().cmp(y.as_bytes()),
        (TypedValue::Bool(x), TypedValue::Bool(y)) => x.cmp(y),
        (TypedValue::Date(x), TypedValue::Date(y)) => x.cmp(y),
        (TypedValue::DateTime(x), TypedValue::DateTime(y)) => x.cmp(y),
        _ => match (num(a), num(b)) {
            // BigDecimal's Ord compares values, whatever the scales.
            (Some(x), Some(y)) => x.cmp(&y),
            _ => panic!("{a:?} and {b:?} do not compare"),
        },
    }
}

/// Every pair: key equality is value equality, key byte order is value
/// order.
fn assert_keys_follow_values(vals: &[TypedValue]) {
    for a in vals {
        for b in vals {
            assert_eq!(
                key(a).as_bytes().cmp(key(b).as_bytes()),
                value_order(a, b),
                "{a:?} vs {b:?}: keys {:?} {:?}",
                key(a),
                key(b)
            );
        }
    }
}

#[test]
fn equal_values_encode_to_identical_keys() {
    let pairs = [
        (int(2), dec("2.0")),
        (dec("2"), dec("2.00")),
        (dec("1.50"), dec("1.5")),
        (neg_zero(), int(0)),
        (neg_zero(), dec("0")),
        (dec("0.000"), dec("0.0")),
        (dec("-12.500"), dec("-12.5")),
        (int(-1000), dec("-1000.000")),
        (dec("100"), dec("100.0")),
    ];
    for (a, b) in &pairs {
        assert_eq!(value_order(a, b), Ordering::Equal, "{a:?} {b:?}");
        assert_eq!(key(a), key(b), "{a:?} {b:?}");
    }
    // A string and its NFD form: "abc" (the same bytes in both forms), and
    // two whose NFD form decomposes.
    for (nfc, nfd) in [
        ("abc", "abc"),
        ("caf\u{e9}", "cafe\u{301}"),
        ("\u{c5}ngstr\u{f6}m", "A\u{30a}ngstro\u{308}m"),
    ] {
        let a = TypedValue::str_nfc(nfc);
        let b = TypedValue::str_nfc(nfd);
        assert_eq!(a, b);
        assert_eq!(key(&a), key(&b), "{nfc:?} {nfd:?}");
    }
}

#[test]
fn non_finite_and_missing_enter_as_null() {
    for v in [
        TypedValue::Missing,
        TypedValue::Nan,
        TypedValue::Inf(true),
        TypedValue::Inf(false),
    ] {
        assert_eq!(order_key(&v), None, "{v:?}");
    }
}

fn numbers() -> Vec<TypedValue> {
    let mut v: Vec<TypedValue> = [
        "-1000000000000000000000000000000000000001",
        "-1000",
        "-999.99",
        "-12.5",
        "-12",
        "-1.25",
        "-1",
        "-0.1",
        "-0.003",
        "-0.0001",
        "0",
        "0.000000000000000000000000000001",
        "0.0001",
        "0.003",
        "0.1",
        "0.12",
        "0.123",
        "0.9999999999999999999999999999999999",
        "1",
        "1.000000000000000000000000000000001",
        "1.5",
        "2",
        "9.99",
        "10",
        "11",
        "100.5",
        "10000000000000000000000000000000000000000",
        "123456789012345678901234567890123456789",
    ]
    .iter()
    .map(|s| dec(s))
    .collect();
    v.extend([int(-7), int(0), int(3), int(1_000_000), int(i64::MAX)]);
    v
}

fn strings() -> Vec<TypedValue> {
    [
        "",
        " ",
        "A",
        "B",
        "Z",
        "a",
        "aa",
        "ab",
        "b",
        "caf\u{e9}",
        "cafe",
        "z",
        "\u{e9}",
        "\u{1F600}",
    ]
    .iter()
    .map(|s| TypedValue::str_nfc(s))
    .collect()
}

fn dates() -> Vec<TypedValue> {
    [
        date(1, 1, 1),
        date(999, 12, 31),
        date(1000, 1, 1),
        date(2026, 2, 3),
        date(2026, 2, 13),
        date(2026, 11, 9),
        date(9999, 12, 31),
    ]
    .into_iter()
    .map(TypedValue::Date)
    .collect()
}

fn datetimes() -> Vec<TypedValue> {
    let d = date(2026, 11, 9);
    vec![
        datetime(date(999, 12, 31), 23, 59, 59, 999_999_999),
        datetime(d, 0, 0, 0, 0),
        datetime(d, 0, 0, 0, 1),
        datetime(d, 0, 0, 0, 500_000_000),
        datetime(d, 0, 0, 1, 0),
        datetime(d, 9, 5, 0, 0),
        datetime(d, 14, 2, 11, 0),
        datetime(d, 23, 59, 59, 999_999_999),
        datetime(date(2026, 11, 10), 0, 0, 0, 0),
    ]
}

#[test]
fn unequal_values_key_order_is_value_order() {
    assert_keys_follow_values(&numbers());
    assert_keys_follow_values(&strings());
    assert_keys_follow_values(&dates());
    assert_keys_follow_values(&datetimes());
}

/// The row order `DuckDB` gives an encoded one-column table under
/// `ORDER BY c0` with the translator's collation (none for `BOOLEAN`).
fn duckdb_order(eng: &mut CheckEngine, name: &str, tag: Tag, vals: &[TypedValue]) -> Vec<usize> {
    let t = Table {
        columns: vec![("v".to_owned(), tag)],
        key: Vec::new(),
        rows: vals.iter().map(|v| vec![v.clone()]).collect(),
    };
    let batch = encode(&t, &tm_checker::table::Extras::default()).unwrap_or_else(|e| panic!("{e}"));
    eng.register(name, batch).unwrap_or_else(|e| panic!("{e}"));
    let collate = if tag == Tag::Bool { "" } else { COLLATE };
    let out = eng
        .query(&format!(
            "SELECT \"row\" FROM \"{name}\" ORDER BY \"c0\" {collate} ASC, \"row\" ASC"
        ))
        .unwrap_or_else(|e| panic!("{e}"));
    rows(&out, 0)
        .unwrap_or_else(|e| panic!("{e}"))
        .into_iter()
        .map(|r| r.unwrap_or_else(|| panic!("null row")))
        .collect()
}

/// The oracle's order of `vals` (stable, so equal values keep input order,
/// as the `row` tie-breaker does).
fn oracle_order(vals: &[TypedValue]) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..vals.len()).collect();
    idx.sort_by(|a, b| value_order(&vals[*a], &vals[*b]));
    idx
}

#[test]
fn duckdb_orders_the_encoded_columns_as_11_18_2_does() {
    let mut eng = CheckEngine::open(&case::settings()).unwrap_or_else(|e| panic!("{e}"));
    // Reverse each sample so the input order is not already the answer, and
    // add an equal pair so ties are exercised.
    let mut nums = numbers();
    nums.extend([dec("2.00"), int(2), neg_zero()]);
    nums.reverse();
    let sets = [
        ("nums", Tag::Dec, nums),
        ("strs", Tag::Str, strings().into_iter().rev().collect()),
        ("dates", Tag::Date, dates().into_iter().rev().collect()),
        (
            "dts",
            Tag::DateTime,
            datetimes().into_iter().rev().collect(),
        ),
        (
            "bools",
            Tag::Bool,
            vec![
                TypedValue::Bool(true),
                TypedValue::Bool(false),
                TypedValue::Bool(true),
            ],
        ),
    ];
    for (name, tag, vals) in &sets {
        assert_eq!(
            duckdb_order(&mut eng, name, *tag, vals),
            oracle_order(vals),
            "{name}"
        );
    }
}

/// A random `dec`: a coefficient of up to 40 digits at a scale from -10 to
/// 45 (11.6 values have scale 0 or more; negative scales are the stronger
/// test and arise inside arithmetic).
fn any_dec() -> impl Strategy<Value = BigDecimal> {
    (
        prop_oneof![
            any::<i64>().prop_map(BigInt::from),
            (-1000i64..1000).prop_map(BigInt::from),
            any::<i128>().prop_map(BigInt::from),
            (any::<i128>(), 0u32..20)
                .prop_map(|(c, k)| BigInt::from(c) * BigInt::from(10u8).pow(k)),
        ],
        -10i64..45,
    )
        .prop_map(|(c, s)| BigDecimal::new(c, s))
}

/// A pair that is often equal in value at different scales.
fn dec_pair() -> impl Strategy<Value = (BigDecimal, BigDecimal)> {
    prop_oneof![
        (any_dec(), any_dec()),
        (any_dec(), 0u32..12).prop_map(|(d, k)| {
            let (c, s) = d.as_bigint_and_exponent();
            let wider = BigDecimal::new(c * BigInt::from(10u8).pow(k), s + i64::from(k));
            (d, wider)
        }),
        // Neighbours: differ in the last place.
        any_dec().prop_map(|d| {
            let (c, s) = d.as_bigint_and_exponent();
            (d, BigDecimal::new(c + 1, s))
        }),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 5000, ..ProptestConfig::default() })]

    #[test]
    fn dec_pairs_key_order_is_value_order((a, b) in dec_pair()) {
        let (x, y) = (TypedValue::Dec(a.clone()), TypedValue::Dec(b.clone()));
        let want = a.cmp(&b);
        prop_assert_eq!(key(&x).as_bytes().cmp(key(&y).as_bytes()), want);
        prop_assert_eq!(key(&x) == key(&y), want == Ordering::Equal);
    }

    #[test]
    fn int_and_dec_pairs_key_order_is_value_order(n in any::<i128>(), b in any_dec()) {
        let (x, y) = (TypedValue::Int(BigInt::from(n)), TypedValue::Dec(b.clone()));
        let want = BigDecimal::new(BigInt::from(n), 0).cmp(&b);
        prop_assert_eq!(key(&x).as_bytes().cmp(key(&y).as_bytes()), want);
    }
}
