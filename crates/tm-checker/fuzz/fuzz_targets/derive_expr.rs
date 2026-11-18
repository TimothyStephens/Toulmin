//! Any text as a `derive.expr` and a `filter.where` over a small fixed
//! table: parsing, rendering, type checking, and evaluation under the 11.6
//! context never panic; a rendered expression parses back to the same
//! tree; a refusal is E6004, E6005, E6006, or E5002.
#![no_main]

use libfuzzer_sys::fuzz_target;
use tm_checker::eval::bind;
use tm_checker::table::Table;
use tm_formats::expr;
use tm_formats::typed_value::{Tag, TypedValue, parse_dec_text, parse_int_text};

fn table() -> Table {
    let dec = |s: &str| parse_dec_text(s).map_or(TypedValue::Missing, TypedValue::Dec);
    let int = |s: &str| parse_int_text(s).map_or(TypedValue::Missing, TypedValue::Int);
    Table {
        columns: vec![
            ("id".into(), Tag::Str),
            ("x".into(), Tag::Dec),
            ("n".into(), Tag::Int),
            ("b".into(), Tag::Bool),
        ],
        key: vec!["id".into()],
        rows: vec![
            vec![
                TypedValue::str_nfc("a"),
                dec("1.50"),
                int("3"),
                TypedValue::Bool(true),
            ],
            vec![
                TypedValue::str_nfc("b"),
                TypedValue::Missing,
                int("-7"),
                TypedValue::Bool(false),
            ],
            vec![
                TypedValue::str_nfc("c"),
                dec("0.000000001"),
                int("0"),
                TypedValue::Missing,
            ],
            vec![
                TypedValue::str_nfc("d"),
                TypedValue::Nan,
                int("123456789012345678901234567890"),
                TypedValue::Bool(true),
            ],
        ],
    }
}

fuzz_target!(|data: &[u8]| {
    let Ok(src) = std::str::from_utf8(data) else {
        return;
    };
    if src.len() > 512 {
        return;
    }
    let Ok(e) = expr::parse(src, "/ops/0/expr") else {
        return;
    };
    let rendered = e.render();
    let again = expr::parse(&rendered, "/ops/0/expr")
        .unwrap_or_else(|err| panic!("{rendered:?} from {src:?} does not parse back: {err}"));
    assert_eq!(again, e, "{rendered:?}");
    let t = table();
    if e.type_of(&|c| t.tag(c), "/ops/0/expr").is_err() {
        return;
    }
    let Ok(b) = bind(&e, &t, "/ops/0/expr") else {
        return;
    };
    for r in 0..t.rows.len() {
        if let Err(err) = b.eval(&t, r, "/ops/0/expr") {
            let c = err.code.as_str();
            assert!(matches!(c, "E5002" | "E6004"), "unexpected code {c}");
        }
    }
});
