//! Item 24: the examples of RFC 8785. Hashed records hold no floats
//! (11.1), so the RFC's examples with fractional or exponent numbers are
//! refused with `E1001`; the parts without them must match the RFC's
//! output byte for byte.

use tm_formats::Code;
use tm_formats::canonical::canonicalize;

/// RFC 8785 section 3.2.2, the input as printed.
const SECTION_3_2_2: &str = r#"{
  "numbers": [333333333.33333329, 1E30, 4.50, 2e-3, 0.000001],
  "string": "\u20ac$\u000F\u000aA'\u0042\u0022\u005c\\\"\/",
  "literals": [null, true, false]
}"#;

#[test]
fn section_3_2_2_numbers_are_refused() {
    assert_eq!(canonicalize(SECTION_3_2_2).unwrap_err().code, Code::E1001);
}

#[test]
fn section_3_2_2_strings_and_literals() {
    let input = r#"{
  "string": "\u20ac$\u000F\u000aA'\u0042\u0022\u005c\\\"\/",
  "literals": [null, true, false]
}"#;
    // The RFC's output with the "numbers" member removed.
    let expected =
        "{\"literals\":[null,true,false],\"string\":\"\u{20ac}$\\u000f\\nA'B\\\"\\\\\\\\\\\"/\"}";
    assert_eq!(canonicalize(input).unwrap(), expected);
}

#[test]
fn section_3_2_3_sorting() {
    let input = r#"{
  "\u20ac": "Euro Sign",
  "\r": "Carriage Return",
  "\ufb33": "Hebrew Letter Dalet With Dagesh",
  "1": "One",
  "\ud83d\ude00": "Emoji: Grinning Face",
  "\u0080": "Control",
  "\u00f6": "Latin Small Letter O With Diaeresis"
}"#;
    let expected = concat!(
        "{\"\\r\":\"Carriage Return\",\"1\":\"One\",\"\u{80}\":\"Control\",",
        "\"\u{f6}\":\"Latin Small Letter O With Diaeresis\",\"\u{20ac}\":\"Euro Sign\",",
        "\"\u{1F600}\":\"Emoji: Grinning Face\",\"\u{fb33}\":\"Hebrew Letter Dalet With Dagesh\"}"
    );
    assert_eq!(canonicalize(input).unwrap(), expected);
}

#[test]
fn appendix_b_integers() {
    // Appendix B values that are integers within plus or minus 2^53 keep
    // the RFC's output; the rest are floats under 11.1.
    for (input, output) in [
        ("0", "0"),
        ("1", "1"),
        ("9007199254740992", "9007199254740992"),
        ("-9007199254740992", "-9007199254740992"),
    ] {
        assert_eq!(canonicalize(input).unwrap(), output);
    }
    for input in [
        "-0",
        "9007199254740993",
        "1e+23",
        "5e-324",
        "0.000001",
        "1e21",
    ] {
        assert_eq!(
            canonicalize(input).unwrap_err().code,
            Code::E1001,
            "{input}"
        );
    }
}
