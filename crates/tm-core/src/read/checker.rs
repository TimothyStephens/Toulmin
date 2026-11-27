//! `checker.preview_steps` and `checker.describe_steps` (section 10; item
//! 78): per-step outputs and counts over the cited tables without
//! recording anything, and the plain-language descriptions with the
//! validation result.

use std::collections::BTreeMap;

use serde_json::{Value, json};
use tm_checker::{InputTable, Request, run};
use tm_formats::id::VersionHandle;
use tm_formats::steps::{Steps, TableSchema};
use tm_formats::typed_value::TypedValue;

use crate::ops::{Core, citable, spec_of, split, table_of};
use crate::{Code, Error, Result};

/// The versions a steps object names, by table name.
fn named_versions(steps: &Value) -> Result<BTreeMap<String, VersionHandle>> {
    let mut out = BTreeMap::new();
    let tables = steps
        .get("tables")
        .and_then(Value::as_object)
        .ok_or_else(|| Error::at(Code::E6002, "/tables", "`tables` is an object"))?;
    for (name, h) in tables {
        let v = h.as_str().and_then(VersionHandle::parse).ok_or_else(|| {
            Error::at(
                Code::E6004,
                format!("/tables/{name}"),
                format!("{h} is not a version handle"),
            )
        })?;
        out.insert(name.clone(), v);
    }
    Ok(out)
}

/// `checker.preview_steps`: run the steps over the cited tables with a
/// trivially true assertion (`count >= 0`) and return every step's table
/// capped at `limit`; nothing is recorded.
///
/// # Errors
/// `E6001` a version not citable or without a spec; the parser's codes;
/// `Error::Index`.
pub fn preview_steps(core: &mut Core, steps: &Value, limit: Option<usize>) -> Result<Value> {
    let versions = named_versions(steps)?;
    let mut tables = BTreeMap::new();
    let (env, index) = split(core);
    let mut tx = index.begin()?;
    for (name, v) in &versions {
        let (file, row) = citable(&mut tx, v)?;
        let (batch, spec, _) = table_of(env, &mut tx, &file.identity, &row)?;
        tables.insert(
            name.clone(),
            InputTable {
                handle: v.to_string(),
                spec,
                batch,
            },
        );
    }
    tx.rollback()?;
    let limit = limit.unwrap_or_else(|| core.settings.rows_per_page());
    let assertion =
        json!({"assert": 1, "kind": "count", "op": "ge", "expected": {"t": "int", "v": "0"}});
    let settings = core.engine_settings();
    let req = Request {
        tables: &tables,
        steps: Some(steps),
        assertion: &assertion,
        citations: &[],
        settings: &settings,
        limit,
    };
    let result = run(&req);
    let rows_json = |rows: &[Vec<TypedValue>]| -> Vec<Value> {
        rows.iter()
            .map(|r| Value::Array(r.iter().map(TypedValue::to_json).collect()))
            .collect()
    };
    Ok(json!({
        "steps": result.steps.iter().enumerate().map(|(i, s)| json!({
            "step": i,
            "op": s.op,
            "description": s.description,
            "columns": s.columns.iter().map(|(n, t)| json!({"name": n, "type": t.as_str()})).collect::<Vec<_>>(),
            "key": s.key,
            "rows": rows_json(&s.rows),
            "row_count": s.row_count,
        })).collect::<Vec<_>>(),
        "error": result.verdict.error.as_ref().map(|e| json!({"code": e.code.as_str(), "field": e.field, "message": e.message})),
        "limit": limit,
    }))
}

/// `checker.describe_steps`: the validation result (the first 11.8 error
/// against the cited specs, or none) and one 11.18.4 description per op.
/// Without data the row counts in the descriptions are 0.
///
/// # Errors
/// `Error::Index`.
pub fn describe_steps(core: &mut Core, steps: &Value) -> Result<Value> {
    let parsed = match Steps::from_json(steps) {
        Ok(s) => s,
        Err(e) => {
            return Ok(
                json!({"valid": false, "error": e.to_json(), "descriptions": Vec::<String>::new()}),
            );
        }
    };
    let mut schemas = BTreeMap::new();
    let mut tx = core.index.begin()?;
    for (name, v) in &parsed.tables {
        if let Some((spec, _)) = spec_of(&mut tx, v.file)? {
            schemas.insert(name.clone(), TableSchema::from_spec(&spec));
        }
    }
    tx.rollback()?;
    let check = parsed.check(&schemas);
    let key: Vec<String> = schemas
        .get(&parsed.from)
        .map(|s| s.key.clone())
        .unwrap_or_default();
    let descriptions: Vec<String> = parsed
        .ops
        .iter()
        .map(|op| tm_checker::describe::describe(op, &key, 0, 0))
        .collect();
    Ok(match check {
        Ok(_) => json!({"valid": true, "error": Value::Null, "descriptions": descriptions}),
        Err(e) => json!({"valid": false, "error": e.to_json(), "descriptions": descriptions}),
    })
}
