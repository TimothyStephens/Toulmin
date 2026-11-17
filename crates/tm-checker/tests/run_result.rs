//! Item 55's result type: step 0 and every step's table capped with its
//! true count, the emphasized rows and columns, the named rows beyond the
//! cap, and an assertion over a `cells` citation with `steps` null.

mod common;

use std::collections::BTreeMap;

use serde_json::{Value, json};
use tm_checker::{InputTable, Outcome, Request, Role};
use tm_formats::citation::Citation;
use tm_formats::events::{EventType, KnownEvent};
use tm_formats::id::VersionHandle;
use tm_formats::parse_spec::ParseSpec;
use tm_formats::typed_value::TypedValue;

use common::case;

fn tables() -> BTreeMap<String, InputTable> {
    let dir = case::checker_root().join("cases/sort_desc_missing_first");
    let spec = ParseSpec::from_json(
        &serde_json::from_str(
            &std::fs::read_to_string(dir.join("specs/de.json")).unwrap_or_default(),
        )
        .unwrap_or_default(),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let batch = tm_files::parse::parse(&dir.join("tables/de.csv"), &spec, &case::settings())
        .unwrap_or_else(|e| panic!("{e}"));
    BTreeMap::from([(
        "de".to_owned(),
        InputTable {
            handle: "F1@v1".to_owned(),
            spec,
            batch,
        },
    )])
}

fn s(v: &str) -> TypedValue {
    TypedValue::str_nfc(v)
}

#[test]
fn capped_steps_named_rows_and_columns() {
    let t = tables();
    let steps = json!({"steps":1,"tables":{"de":"F1@v1"},"from":"de","ops":[
        {"op":"sort","v":1,"by":[{"column":"fc","dir":"desc"}],"missing":"last"}
    ]});
    let assertion = json!({"assert":1,"kind":"compare_rows",
        "left":{"row":[{"t":"str","v":"EGFR"}],"column":"fc"},
        "right":{"row":[{"t":"str","v":"MDM2"}],"column":"fc"},"op":"gt"});
    let citations = vec![Citation::Cells {
        version: VersionHandle::parse("F1@v1").unwrap_or_else(|| panic!("handle")),
        keys: vec![vec![s("TP53")]],
        columns: Some(vec!["p".to_owned(), "fc".to_owned(), "nope".to_owned()]),
    }];
    let settings = case::settings();
    let r = tm_checker::run(&Request {
        tables: &t,
        steps: Some(&steps),
        assertion: &assertion,
        citations: &citations,
        settings: &settings,
        limit: 2,
    });
    assert_eq!(r.verdict.outcome, Outcome::Holds, "{:?}", r.verdict.error);
    // Step 0 then the sort, each capped at 2 with the true count 5.
    assert_eq!(r.steps.len(), 2);
    assert!(r.steps[0].op.is_none() && r.steps[0].description.is_none());
    assert_eq!(r.steps[1].op, Some("sort"));
    for st in &r.steps {
        assert_eq!(st.rows.len(), 2);
        assert_eq!(st.row_count, 5);
    }
    // Final order: EGFR 3.0, TP53 2.5, BRCA1 0.5, MDM2 -1.5, KRAS missing.
    assert_eq!(r.emphasized_rows.len(), 2);
    assert_eq!(r.emphasized_rows[0].role, Role::Left);
    assert_eq!(r.emphasized_rows[0].position, Some(1));
    assert_eq!(r.emphasized_rows[1].role, Role::Right);
    assert_eq!(r.emphasized_rows[1].position, Some(4));
    assert_eq!(r.beyond_cap.len(), 1);
    assert_eq!(r.beyond_cap[0].role, Role::Right);
    assert_eq!((r.beyond_cap[0].position, r.beyond_cap[0].of), (4, 5));
    assert_eq!(r.beyond_cap[0].row[0], s("MDM2"));
    // The assertion's column, then the citation's, each once, if present.
    assert_eq!(r.emphasized_columns, vec!["fc".to_owned(), "p".to_owned()]);
    // Capped: the full final rows are not carried; the hash is.
    assert!(r.final_rows.is_none());
    assert!(r.verdict.final_rows_hash.is_some());
    assert_eq!(r.verdict.key_map.as_ref().map(Vec::len), Some(5));
}

#[test]
fn steps_null_takes_the_cited_rows_in_key_order() {
    let t = tables();
    let assertion = json!({"assert":1,"kind":"value","row":[{"t":"str","v":"TP53"}],
        "column":"fc","op":"eq","expected":{"t":"dec","v":"2.5"}});
    let citations = vec![Citation::Cells {
        version: VersionHandle::parse("F1@v1").unwrap_or_else(|| panic!("handle")),
        keys: vec![vec![s("TP53")], vec![s("BRCA1")]],
        columns: None,
    }];
    let settings = case::settings();
    let r = tm_checker::run(&Request {
        tables: &t,
        steps: None,
        assertion: &assertion,
        citations: &citations,
        settings: &settings,
        limit: 100,
    });
    assert_eq!(r.verdict.outcome, Outcome::Holds, "{:?}", r.verdict.error);
    assert_eq!(r.verdict.steps, Vec::new());
    assert_eq!(r.steps.len(), 1);
    let rows = r.final_rows.unwrap_or_default();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0][0], s("BRCA1"));
    assert_eq!(rows[1][0], s("TP53"));
    let body = r.verdict.body_members();
    assert_eq!(body["assertion"]["outcome"], "holds");
    assert_eq!(
        body["assertion"]["observed"],
        json!({"cell":{"t":"dec","v":"2.5"}})
    );
}

/// The Checker's body members plus the four the Claims library adds, as one
/// `claim.checked` body, checked by tm-formats (11.16).
fn checked_body(r: &tm_checker::RunResult) -> KnownEvent {
    let mut body = r.verdict.body_members();
    body.insert("ref".into(), json!("C1"));
    body.insert(
        "revision".into(),
        json!("sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"),
    );
    body.insert("against".into(), json!(["F1@v1"]));
    body.insert(
        "evidence".into(),
        json!({"outcome": r.verdict.outcome.as_str(), "detail": null}),
    );
    KnownEvent::new(EventType::ClaimChecked, &Value::Object(body)).unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn error_verdict_nulls_the_hashes_and_key_map_and_the_body_is_valid() {
    let t = tables();
    let assertion = json!({"assert":1,"kind":"value","row":[{"t":"str","v":"TP53"}],
        "column":"nope","op":"eq","expected":{"t":"dec","v":"2.5"}});
    let steps = json!({"steps":1,"tables":{"de":"F1@v1"},"from":"de","ops":[
        {"op":"sort","v":1,"by":[{"column":"fc","dir":"desc"}],"missing":"last"}
    ]});
    let settings = case::settings();
    let r = tm_checker::run(&Request {
        tables: &t,
        steps: Some(&steps),
        assertion: &assertion,
        citations: &[],
        settings: &settings,
        limit: 100,
    });
    assert_eq!(r.verdict.outcome, Outcome::Error);
    let body = r.verdict.body_members();
    assert_eq!(body["final_rows_hash"], Value::Null);
    assert_eq!(body["key_map_hash"], Value::Null);
    assert_eq!(body["key_map"], Value::Null);
    assert!(body["error"]["code"].is_string());
    checked_body(&r);
}

#[test]
fn holds_verdict_body_is_valid() {
    let t = tables();
    let assertion = json!({"assert":1,"kind":"value","row":[{"t":"str","v":"TP53"}],
        "column":"fc","op":"eq","expected":{"t":"dec","v":"2.5"}});
    let steps = json!({"steps":1,"tables":{"de":"F1@v1"},"from":"de","ops":[
        {"op":"sort","v":1,"by":[{"column":"fc","dir":"desc"}],"missing":"last"}
    ]});
    let settings = case::settings();
    let r = tm_checker::run(&Request {
        tables: &t,
        steps: Some(&steps),
        assertion: &assertion,
        citations: &[],
        settings: &settings,
        limit: 100,
    });
    assert_eq!(r.verdict.outcome, Outcome::Holds, "{:?}", r.verdict.error);
    assert!(r.verdict.body_members()["key_map"].is_array());
    checked_body(&r);
}
