//! Running one conformance case directory (item 92), shared by
//! `tests/conformance.rs` and `examples/refcheck.rs`. The output has the
//! shape of `expected.json`: `{verdict, steps, columns, final_rows,
//! final_rows_hash, key_map_hash, error}` for a case under `cases/`, and
//! `{verdict, error}` for one under `assertions/eval/`.

#![allow(dead_code, reason = "each includer uses part of it")]

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{Value, json};

use tm_checker::{InputTable, Outcome, Request, RunResult};
use tm_duckdb::EngineSettings;
use tm_formats::parse_spec::ParseSpec;

fn read_json(p: &Path) -> Result<Value, String> {
    let text = std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", p.display()))
}

fn error_json(code: &str, field: Option<&str>) -> Value {
    json!({"code": code, "field": field})
}

/// The engine settings a case runs under: the defaults, with the temp
/// directory under the system temp directory.
pub fn settings() -> EngineSettings {
    EngineSettings::default()
}

/// Run a `cases/<name>/` directory; returns the result too, for the
/// snapshot test. A table that `tm-files` refuses with a catalog error
/// (`E3102` for a cell, field `null`, 11.7 and 11.12) is the case's error
/// verdict with that code and field, and the Checker does not run (`None`).
pub fn run_case(dir: &Path) -> Result<(Value, Option<RunResult>), String> {
    let settings = settings();
    let steps = read_json(&dir.join("steps.json"))?;
    let assertion = read_json(&dir.join("assertion.json"))?;
    let handles: BTreeMap<String, String> = steps
        .get("tables")
        .and_then(Value::as_object)
        .map(|m| {
            m.iter()
                .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("F0@v1").to_owned()))
                .collect()
        })
        .unwrap_or_default();
    let mut tables = BTreeMap::new();
    let mut names: Vec<_> = std::fs::read_dir(dir.join("tables"))
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .collect();
    names.sort();
    for path in names {
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or("table file name")?
            .to_owned();
        let spec =
            ParseSpec::from_json(&read_json(&dir.join("specs").join(format!("{stem}.json")))?)
                .map_err(|e| format!("spec {stem}: {e}"))?;
        let batch = match tm_files::parse::parse(&path, &spec, &settings) {
            Ok(b) => b,
            Err(tm_files::Error::Format(e)) => {
                return Ok((
                    json!({
                        "verdict": Outcome::Error.as_str(),
                        "steps": null, "columns": null, "final_rows": null,
                        "final_rows_hash": null, "key_map_hash": null,
                        "error": error_json(e.code.as_str(), e.field.as_deref()),
                    }),
                    None,
                ));
            }
            Err(e) => return Err(format!("parse {stem}: {e}")),
        };
        let handle = handles
            .get(&stem)
            .cloned()
            .unwrap_or_else(|| "F0@v1".to_owned());
        tables.insert(
            stem,
            InputTable {
                handle,
                spec,
                batch,
            },
        );
    }
    let req = Request {
        tables: &tables,
        steps: Some(&steps),
        assertion: &assertion,
        citations: &[],
        settings: &settings,
        limit: usize::MAX,
    };
    let r = tm_checker::run(&req);
    Ok((observed(&r), Some(r)))
}

/// The `expected.json`-shaped view of a result.
pub fn observed(r: &RunResult) -> Value {
    let v = &r.verdict;
    if let Some(e) = &v.error {
        return json!({
            "verdict": v.outcome.as_str(),
            "steps": null, "columns": null, "final_rows": null,
            "final_rows_hash": null, "key_map_hash": null,
            "error": error_json(e.code.as_str(), e.field.as_deref()),
        });
    }
    let columns = r.final_columns.clone().unwrap_or_default();
    let rows: Vec<Value> = r
        .final_rows
        .as_ref()
        .map(|rows| {
            rows.iter()
                .map(|row| {
                    Value::Object(
                        columns
                            .iter()
                            .zip(row)
                            .map(|(c, v)| (c.clone(), v.to_json()))
                            .collect(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    json!({
        "verdict": v.outcome.as_str(),
        "steps": v.steps.iter().map(|s| json!({"rows": s.rows_out, "description": s.description})).collect::<Vec<_>>(),
        "columns": columns,
        "final_rows": rows,
        "final_rows_hash": v.final_rows_hash,
        "key_map_hash": v.key_map_hash,
        "error": null,
    })
}

/// Run an `assertions/eval/<name>/` directory.
pub fn run_eval(dir: &Path) -> Result<Value, String> {
    let case = read_json(&dir.join("case.json"))?;
    Ok(
        match tm_checker::run::evaluate_located(&case["input"], &case["observed"]) {
            Ok(o) => json!({"verdict": o.as_str(), "error": null}),
            Err(e) => json!({
                "verdict": Outcome::Error.as_str(),
                "error": error_json(e.code.as_str(), e.field.as_deref()),
            }),
        },
    )
}

/// The `expected.json` of a case directory.
pub fn expected(dir: &Path) -> Result<Value, String> {
    read_json(&dir.join("expected.json"))
}

/// Case directories under `root`, sorted by name.
pub fn case_dirs(root: &Path) -> Vec<std::path::PathBuf> {
    let mut v: Vec<_> = std::fs::read_dir(root)
        .map(|it| {
            it.filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

/// The workspace's `conformance/checker/` directory.
pub fn checker_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance/checker")
}
