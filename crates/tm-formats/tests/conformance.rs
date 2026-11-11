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
use tm_formats::assertion::Assertion;
use tm_formats::canonical;
use tm_formats::citation::Citation;
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

/// A `table` member of an assertion case: `{"columns": [[name, type], ...], "key": [...]}`.
fn table_schema(t: &Value) -> TableSchema {
    let columns = t["columns"]
        .as_array()
        .map(|cols| {
            cols.iter()
                .filter_map(|c| {
                    let name = c[0].as_str()?;
                    let ty = SpecType::parse(c[1].as_str()?)?;
                    Some((name.to_owned(), ty.tag()))
                })
                .collect()
        })
        .unwrap_or_default();
    let key = t["key"]
        .as_array()
        .map(|k| {
            k.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    TableSchema { columns, key }
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
                    // A str that was not NFC reads as its NFC form (11.6);
                    // every other valid value round-trips to its input.
                    let want = expect.get("normalized").unwrap_or(&case["input"]);
                    assert_eq!(&v.to_json(), want, "typed values round-trip");
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
            // Schema only (11.9). Evaluation cases live under
            // conformance/checker/assertions/eval/ and are a P4 fact.
            if case.get("observed").is_some() {
                return Err("an evaluation case does not belong under formats".into());
            }
            let got = Assertion::from_json(&case["input"]).and_then(|a| match case.get("table") {
                Some(t) => a.check(&table_schema(t)),
                None => Ok(()),
            });
            if expect.get("valid").is_some() {
                got.map_err(|e| e.to_string())
            } else {
                expect_error(expect, got)
            }
        }
        "parse_spec" => {
            let got = ParseSpec::from_json(&case["input"]);
            if expect.get("code").is_some() && case.get("header").is_none() {
                return expect_error(expect, got.map(|_| ()));
            }
            let spec = got.map_err(|e| format!("rejected: {e}"))?;
            if let Some(h) = case.get("header") {
                let header: Vec<&str> = h
                    .as_array()
                    .ok_or("header")?
                    .iter()
                    .filter_map(Value::as_str)
                    .collect();
                let got = spec.check_header(&header);
                return if expect.get("valid").is_some() {
                    got.map_err(|e| e.to_string())
                } else {
                    expect_error(expect, got)
                };
            }
            if Some(spec.fingerprint().as_str()) != expect["fingerprint"].as_str() {
                return Err(format!("fingerprint {}", spec.fingerprint()));
            }
            Ok(())
        }
        "citations" => match case["case"].as_str() {
            Some("json") => {
                let got = Citation::from_json(&case["input"], "");
                if expect.get("valid").is_some() {
                    let c = got.map_err(|e| e.to_string())?;
                    if c.to_json() != case["input"] {
                        return Err("the citation does not round-trip".into());
                    }
                    Ok(())
                } else {
                    expect_error(expect, got.map(|_| ()))
                }
            }
            Some("cli") => {
                let text = case["input"].as_str().ok_or("input")?;
                let spec = match case.get("spec") {
                    Some(s) if !s.is_null() => {
                        Some(ParseSpec::from_json(s).map_err(|e| e.to_string())?)
                    }
                    _ => None,
                };
                let got = Citation::parse_cli(text, spec.as_ref());
                if let Some(want) = expect.get("citation") {
                    let c = got.map_err(|e| e.to_string())?;
                    if &c.to_json() != want {
                        return Err(format!("parsed as {}", c.to_json()));
                    }
                    Ok(())
                } else {
                    expect_error(expect, got.map(|_| ()))
                }
            }
            other => Err(format!("case {other:?}")),
        },
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
        "citations",
        "envelope",
        "expr",
        "ids",
        "parse_spec",
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
