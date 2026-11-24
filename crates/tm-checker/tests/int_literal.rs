//! Owner ruling of 2026-11-24 (after ruling 56): an `INT` literal of 6,145
//! digits is in range (adjusted exponent 6144) and evaluates exactly. The
//! case under `tests/cases/int_literal_6145_digits/` derives `i + 9…9`
//! (6,145 nines); its `expected.json` is the Python reference checker's
//! output, which CI's checker loop also compares, so passing here means
//! both implementations give the same values and hashes.

mod common;

use std::path::Path;

use common::case;

#[test]
fn int_literal_of_6145_digits_matches_the_reference() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/cases/int_literal_6145_digits");
    let (got, _) = case::run_case(&dir).unwrap_or_else(|e| panic!("{e}"));
    let expected = case::expected(&dir).unwrap_or_else(|e| panic!("{e}"));
    let y = &got["final_rows"][0]["y"];
    assert_eq!(y["t"], "int");
    assert_eq!(
        y["v"].as_str().map(str::len),
        Some(6146),
        "1 + (10^6145 - 1) is 10^6145"
    );
    assert_eq!(got, expected);
}
