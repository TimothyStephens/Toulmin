//! Owner rulings of 2026-11-24 (after ruling 56): an `int` of more than
//! 4,300 digits, the CPython int-string limit, is read and written exactly.
//! Each case under `tests/cases/` has an `expected.json` that is the Python
//! reference checker's output, which CI's checker loop also compares, so
//! passing here means both implementations give the same values, verdict,
//! and hashes.
//!
//! - `int_literal_6145_digits`: `i + 9…9` (6,145 nines), an in-range `INT`
//!   literal; `y` for `i` = 1 is 10^6145.
//! - `int_expected_6145_digits`: a `value` assertion whose `expected` is
//!   the 6,145-digit `int` 10^6144; holds.

mod common;

use std::path::Path;

use common::case;

fn cases_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/cases")
}

#[test]
fn every_big_int_case_matches_the_reference() {
    let dirs = case::case_dirs(&cases_root());
    assert_eq!(dirs.len(), 2, "{dirs:?}");
    for dir in &dirs {
        let (got, _) = case::run_case(dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
        let expected = case::expected(dir).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(got, expected, "{}", dir.display());
    }
}

#[test]
fn int_literal_of_6145_digits_gives_10_pow_6145() {
    let (got, _) = case::run_case(&cases_root().join("int_literal_6145_digits"))
        .unwrap_or_else(|e| panic!("{e}"));
    let y = &got["final_rows"][0]["y"];
    assert_eq!(y["t"], "int");
    assert_eq!(y["v"].as_str().map(str::len), Some(6146));
}

#[test]
fn int_expected_of_6145_digits_holds() {
    let (got, _) = case::run_case(&cases_root().join("int_expected_6145_digits"))
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(got["verdict"], "holds");
}
