//! Item 23: every case under `conformance/formats/` (case format in the
//! crate's README.md), and every `steps/invalid/` case rejected with
//! the named code and field.
//!
//! With `TM_FORMATS_IDS_OUT` set, also writes one line per case file,
//! `<path> <file id> <subject>`, sorted by path, which
//! `reference/formats/test_formats.py` writes in the same form; the P1
//! checkpoint compares the two files.

#![allow(
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::result_large_err,
    reason = "a test harness: one dispatch over case kinds, panics report setup failures"
)]

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use serde_json::Value;
use tm_formats::Error;
use tm_formats::assertion::{Assertion, Observed};
use tm_formats::canonical;
use tm_formats::envelope;
use tm_formats::events::Event;
use tm_formats::expr;
use tm_formats::id::{genesis, id_of};
use tm_formats::parse_spec::ParseSpec;
use tm_formats::steps::{Steps, TableSchema};
use tm_formats::typed_value::{
    SpecType, TypedValue, dec_from_f64_round15, dec_from_f64_shortest, dec_text,
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance/formats")
}

fn files(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .expect("read dir")
        .map(|e| e.expect("entry").path())
        .collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            files(&p, out);
        } else if p.extension().is_some_and(|e| e == "json") {
            out.push(p);
        }
    }
}

/// `{"code","field"}` from an error, for comparison with `expect`.
fn code_field(e: &Error) -> (String, Value) {
    (
        e.code.as_str().to_owned(),
        e.field.clone().map_or(Value::Null, Value::String),
    )
}

fn expect_error(expect: &Value, got: Result<(), Error>) -> Result<(), String> {
    let want = (
        expect["code"].as_str().unwrap_or("").to_owned(),
        expect.get("field").cloned().unwrap_or(Value::Null),
    );
    match got {
        Ok(()) => Err(format!("accepted; expected {} at {}", want.0, want.1)),
        Err(e) if code_field(&e) == want => Ok(()),
        Err(e) => Err(format!(
            "got {} at {:?} ({}); expected {} at {}",
            e.code, e.field, e.message, want.0, want.1
        )),
    }
}

fn specs(case: &Value) -> Result<BTreeMap<String, TableSchema>, String> {
    let mut out = BTreeMap::new();
    if let Some(m) = case.get("specs").and_then(Value::as_object) {
        for (name, s) in m {
            let spec = ParseSpec::from_json(s).map_err(|e| format!("spec {name}: {e}"))?;
            out.insert(name.clone(), TableSchema::from_spec(&spec));
        }
    }
    Ok(out)
}

fn typed_opt(v: &Value) -> Result<Option<TypedValue>, String> {
    if v.is_null() {
        Ok(None)
    } else {
        TypedValue::from_json(v, "")
            .map(Some)
            .map_err(|e| e.to_string())
    }
}

fn run_case(kind: &str, case: &Value) -> Result<(), String> {
    let expect = &case["expect"];
    match kind {
        "canonical" => {
            let text = case["input_text"].as_str().ok_or("input_text")?;
            match canonical::parse(text).and_then(|v| Ok((canonical::canonical(&v)?, id_of(&v)?))) {
                Ok((c, id)) => {
                    if expect.get("code").is_some() {
                        return Err(format!("accepted; expected {}", expect["code"]));
                    }
                    if c != expect["canonical"].as_str().unwrap_or("")
                        || id.as_str() != expect["id"].as_str().unwrap_or("")
                    {
                        return Err(format!("canonical {c:?} id {id}"));
                    }
                    Ok(())
                }
                Err(e) if Some(e.code.as_str()) == expect["code"].as_str() => Ok(()),
                Err(e) => Err(format!("got {e}; expected {expect}")),
            }
        }
        "ids" => {
            let id = if let Some(g) = case.get("genesis") {
                genesis(
                    g["chain"].as_str().unwrap_or(""),
                    g["project_id"].as_str().unwrap_or(""),
                )
            } else {
                id_of(&case["input"]).map_err(|e| e.to_string())?
            };
            if Some(id.as_str()) == expect["id"].as_str() {
                Ok(())
            } else {
                Err(format!("id {id}"))
            }
        }
        "envelope" => {
            let line = case["line"].as_str().ok_or("line")?;
            let got = envelope::read_line(line);
            if expect.get("code").is_some() {
                return expect_error(expect, got.map(|_| ()));
            }
            let l = got.map_err(|e| format!("rejected: {e}"))?;
            if Some(l.id.as_str()) != expect["id"].as_str() {
                return Err(format!("id {}", l.id));
            }
            let (event, ty) = match &l.event {
                Event::Known(k) => ("known", k.ty().name().to_owned()),
                Event::Opaque(o) => ("opaque", o.ty.clone()),
            };
            if Some(event) != expect["event"].as_str()
                || Some(ty.as_str()) != expect["type"].as_str()
            {
                return Err(format!("read as {event} {ty}"));
            }
            Ok(())
        }
        "typed_values" => match case["case"].as_str() {
            Some("typed_value") => {
                let got = TypedValue::from_json(&case["input"], "").map(|v| {
                    assert_eq!(v.to_json(), case["input"], "typed values round-trip");
                });
                if expect.get("valid").is_some() {
                    got.map_err(|e| e.to_string())
                } else {
                    expect_error(expect, got)
                }
            }
            Some("from_f64") => {
                let bits = case["input"]["f64"]
                    .as_str()
                    .and_then(|s| s.strip_prefix("0x"))
                    .ok_or("f64")?;
                let x = f64::from_bits(u64::from_str_radix(bits, 16).map_err(|e| e.to_string())?);
                let d = match case["input"]["rule"].as_str() {
                    Some("round15") => dec_from_f64_round15(x),
                    Some("shortest") => dec_from_f64_shortest(x),
                    other => return Err(format!("rule {other:?}")),
                };
                let got = d.map(|d| dec_text(&d)).unwrap_or_default();
                if Some(got.as_str()) == expect["dec"].as_str() {
                    Ok(())
                } else {
                    Err(format!("dec {got}"))
                }
            }
            other => Err(format!("case kind {other:?}")),
        },
        "steps" => {
            let schemas = specs(case)?;
            let got = Steps::from_json(&case["input"])
                .and_then(|s| s.check(&schemas))
                .map(|_| ());
            if expect.get("valid").is_some() {
                got.map_err(|e| e.to_string())
            } else {
                expect_error(expect, got)
            }
        }
        "expr" => {
            let src = case["input"].as_str().ok_or("input")?;
            let cols: BTreeMap<String, SpecType> = case["columns"]
                .as_object()
                .ok_or("columns")?
                .iter()
                .map(|(k, v)| {
                    (
                        k.clone(),
                        SpecType::parse(v.as_str().unwrap_or("")).unwrap_or(SpecType::Str),
                    )
                })
                .collect();
            let lookup = |c: &str| cols.get(c).map(|t| t.tag());
            let got = expr::parse(src, "").and_then(|e| Ok((e.render(), e.type_of(&lookup, "")?)));
            if expect.get("code").is_some() {
                return expect_error(expect, got.map(|_| ()));
            }
            let (render, ty) = got.map_err(|e| e.to_string())?;
            let ty = ty.map_or(Value::Null, |t| Value::String(t.as_str().into()));
            if Some(render.as_str()) != expect["render"].as_str() || ty != expect["type"] {
                return Err(format!("render {render:?} type {ty}"));
            }
            Ok(())
        }
        "assertions" => {
            let parsed = Assertion::from_json(&case["input"]);
            let Some(obs) = case.get("observed") else {
                let got = parsed.map(|_| ());
                return if expect.get("valid").is_some() {
                    got.map_err(|e| e.to_string())
                } else {
                    expect_error(expect, got)
                };
            };
            let a = parsed.map_err(|e| format!("rejected: {e}"))?;
            let (c1, c2);
            let observed = if let Some(c) = obs.get("cell") {
                c1 = typed_opt(c)?;
                Observed::Cell(c1.as_ref())
            } else if let Some(p) = obs.get("position") {
                Observed::Position(p.as_u64())
            } else if let Some(n) = obs.get("count") {
                Observed::Count(n.as_u64().ok_or("count")?)
            } else if let Some(p) = obs.get("present") {
                Observed::Present(p.as_bool().ok_or("present")?)
            } else {
                c1 = typed_opt(&obs["left"])?;
                c2 = typed_opt(&obs["right"])?;
                Observed::Cells(c1.as_ref(), c2.as_ref())
            };
            let got = a.evaluate(observed);
            if let Some(o) = expect.get("outcome") {
                let got = got.map_err(|e| e.to_string())?;
                if Some(got.as_str()) == o.as_str() {
                    Ok(())
                } else {
                    Err(format!("outcome {}", got.as_str()))
                }
            } else {
                expect_error(expect, got.map(|_| ()))
            }
        }
        other => Err(format!("unknown case directory {other}")),
    }
}

/// The subject ID of a case: what both implementations can compute.
fn subject(kind: &str, case: &Value) -> String {
    let id_or_code = |r: Result<tm_formats::id::EventId, Error>| {
        r.map_or_else(|e| e.code.as_str().to_owned(), |i| i.as_str().to_owned())
    };
    match kind {
        "canonical" => id_or_code(
            canonical::parse(case["input_text"].as_str().unwrap_or("")).and_then(|v| id_of(&v)),
        ),
        "ids" => match case.get("genesis") {
            Some(g) => genesis(
                g["chain"].as_str().unwrap_or(""),
                g["project_id"].as_str().unwrap_or(""),
            )
            .as_str()
            .to_owned(),
            None => id_or_code(id_of(&case["input"])),
        },
        "envelope" => id_or_code(
            canonical::parse(case["line"].as_str().unwrap_or("")).and_then(|mut v| {
                if let Some(m) = v.as_object_mut() {
                    m.shift_remove("id");
                }
                id_of(&v)
            }),
        ),
        _ => id_or_code(id_of(&case["input"])),
    }
}

#[test]
fn conformance_formats() {
    let root = root();
    let mut paths = Vec::new();
    files(&root, &mut paths);
    let mut failures = Vec::new();
    let mut per_kind: BTreeMap<String, usize> = BTreeMap::new();
    let mut listing = String::new();
    for path in &paths {
        let rel = path
            .strip_prefix(&root)
            .expect("under root")
            .to_string_lossy()
            .replace('\\', "/");
        let kind = rel.split('/').next().unwrap_or("").to_owned();
        let text = std::fs::read_to_string(path).expect("read case");
        let case = match canonical::parse(&text) {
            Ok(c) => c,
            Err(e) => {
                failures.push(format!("{rel}: case file is not acceptable JSON: {e}"));
                continue;
            }
        };
        *per_kind.entry(kind.clone()).or_default() += 1;
        if let Err(msg) = run_case(&kind, &case) {
            failures.push(format!("{rel}: {msg}"));
        }
        let file_id = id_of(&case).expect("case file canonicalizes");
        writeln!(listing, "{rel} {file_id} {}", subject(&kind, &case)).expect("write");
    }
    if let Some(out) = std::env::var_os("TM_FORMATS_IDS_OUT") {
        std::fs::write(out, &listing).expect("write TM_FORMATS_IDS_OUT");
    }
    for k in [
        "assertions",
        "canonical",
        "envelope",
        "expr",
        "ids",
        "steps",
        "typed_values",
    ] {
        assert!(
            per_kind.get(k).copied().unwrap_or(0) > 0,
            "no cases under {k}/"
        );
    }
    println!("{} cases: {per_kind:?}", paths.len());
    assert!(
        failures.is_empty(),
        "{} failing cases:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn every_steps_invalid_case_names_code_and_field() {
    let mut paths = Vec::new();
    files(&root().join("steps/invalid"), &mut paths);
    assert_ne!(paths.len(), 0);
    for p in paths {
        let case: Value =
            canonical::parse(&std::fs::read_to_string(&p).expect("read")).expect("parse");
        let e = &case["expect"];
        assert!(
            e["code"].as_str().is_some_and(|c| c.starts_with('E')),
            "{}: no code",
            p.display()
        );
        assert!(e["field"].as_str().is_some(), "{}: no field", p.display());
    }
}

#[test]
fn every_gate_a_type_has_a_valid_and_an_invalid_line() {
    for ty in tm_formats::events::EventType::all() {
        for suffix in ["valid", "invalid"] {
            let p = root().join(format!("envelope/{}.{suffix}.json", ty.name()));
            assert!(p.exists(), "missing {}", p.display());
        }
    }
    // Every 11.23 citation kind appears in at least one valid claim.added case.
    let mut kinds = std::collections::BTreeSet::new();
    let mut paths = Vec::new();
    files(&root().join("envelope"), &mut paths);
    for p in paths.iter().filter(|p| {
        p.to_string_lossy().contains("claim.added") && p.to_string_lossy().ends_with(".valid.json")
    }) {
        let case: Value =
            canonical::parse(&std::fs::read_to_string(p).expect("read")).expect("parse");
        let line: Value =
            canonical::parse(case["line"].as_str().expect("line")).expect("line parses");
        for c in line["body"]["citations"].as_array().expect("citations") {
            kinds.insert(c["kind"].as_str().expect("kind").to_owned());
        }
    }
    let all: std::collections::BTreeSet<String> = [
        "cells",
        "whole_file",
        "page",
        "panel",
        "quote",
        "page_region",
        "image_region",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();
    assert_eq!(kinds, all);
}
