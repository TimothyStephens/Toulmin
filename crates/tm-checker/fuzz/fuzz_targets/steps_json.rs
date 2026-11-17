//! Any bytes as a steps object and an assertion: reading and the schema
//! checks never panic, and a refusal always carries an 11.8 or 11.9 code.
#![no_main]

use std::collections::BTreeMap;

use libfuzzer_sys::fuzz_target;
use serde_json::{Value, json};
use tm_formats::steps::TableSchema;
use tm_formats::typed_value::Tag;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let (steps_text, assertion_text) = text.split_once('\u{0}').unwrap_or((text, "{}"));
    let Ok(steps) = serde_json::from_str::<Value>(steps_text) else {
        return;
    };
    let assertion =
        serde_json::from_str::<Value>(assertion_text).unwrap_or_else(|_| json!({"assert": 1}));
    let schema = TableSchema {
        columns: vec![
            ("id".into(), Tag::Str),
            ("x".into(), Tag::Dec),
            ("n".into(), Tag::Int),
            ("d".into(), Tag::Date),
            ("b".into(), Tag::Bool),
        ],
        key: vec!["id".into()],
    };
    let schemas = BTreeMap::from([("t".to_owned(), schema.clone()), ("u".to_owned(), schema)]);
    if let Err(e) = tm_checker::rules::validate(Some(&steps), &assertion, &schemas, None) {
        let c = e.code.as_str();
        assert!(
            matches!(c, "E1001" | "E1004" | "E6002" | "E6004" | "E6005" | "E6006"),
            "unexpected code {c}"
        );
    }
});
