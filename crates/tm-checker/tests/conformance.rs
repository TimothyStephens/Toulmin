//! Item 59: every case under `conformance/checker/cases/` and
//! `conformance/checker/assertions/eval/` against its `expected.json`
//! (Verification Plan, Conformance: checker). A case with an open entry in
//! `docs/decisions/checker-diffs.md` is listed in `OPEN` and must still
//! differ; when it starts to agree the test fails, so the list is kept
//! true.

mod common;

use std::fmt::Write as _;

use common::case;

/// Cases with an open `checker-diffs.md` entry, awaiting a ruling.
const OPEN: &[&str] = &[];

fn diff(expected: &serde_json::Value, got: &serde_json::Value) -> String {
    let mut out = String::new();
    if let (Some(e), Some(g)) = (expected.as_object(), got.as_object()) {
        for (k, ev) in e {
            let gv = g.get(k).unwrap_or(&serde_json::Value::Null);
            if ev != gv {
                let _ = write!(out, "  {k}:\n    expected {ev}\n    got      {gv}\n");
            }
        }
    }
    out
}

fn check_dir(root: &std::path::Path, eval: bool) -> (usize, Vec<String>) {
    let mut failures = Vec::new();
    let dirs = case::case_dirs(root);
    for dir in &dirs {
        let name = dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("?")
            .to_owned();
        let got = if eval {
            case::run_eval(dir)
        } else {
            case::run_case(dir).map(|(v, _)| v)
        };
        let expected = case::expected(dir);
        let same = match (&got, &expected) {
            (Ok(g), Ok(e)) => g == e,
            _ => false,
        };
        let open = OPEN.contains(&name.as_str());
        match (same, open) {
            (true, false) | (false, true) => {}
            (true, true) => failures.push(format!(
                "{name}: listed as open in checker-diffs.md but now agrees; close the entry"
            )),
            (false, false) => failures.push(match (got, expected) {
                (Ok(g), Ok(e)) => format!("{name}:\n{}", diff(&e, &g)),
                (Err(e), _) | (_, Err(e)) => format!("{name}: {e}"),
            }),
        }
    }
    (dirs.len(), failures)
}

#[test]
fn checker_cases() {
    let (n, failures) = check_dir(&case::checker_root().join("cases"), false);
    assert!(n >= 60, "item 92 asks for at least 60 cases; found {n}");
    assert!(
        failures.is_empty(),
        "{} of {n} cases differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn assertion_eval_cases() {
    let (n, failures) = check_dir(&case::checker_root().join("assertions/eval"), true);
    assert_eq!(n, 28);
    assert!(
        failures.is_empty(),
        "{} of {n} cases differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
