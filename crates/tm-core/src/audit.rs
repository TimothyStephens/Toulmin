//! `tm audit` orchestration (item 80; CUJ 8; G13; section 9's `audit`
//! row): verify the chain and the kept objects, re-hash every registered
//! version's file, re-run every claim check and compare it with the stored
//! verdict, list the open items, write the report as
//! `audit/<report_hash>.json` and `audit/<report_hash>.html`, and record
//! `audit.completed`.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};
use tm_formats::canonical;
use tm_formats::events::{EventType, KnownEvent};
use tm_formats::id::{EventId, id_of};
use tm_index::Param;
use tm_owner_cap::OwnerCap;
use tm_record::Finding;

use crate::ops::claim_check::check;
use crate::ops::file_register::candidate;
use crate::ops::{Core, files, latest_version, owner};
use crate::read::Flag;
use crate::read::claims::{claim_rows, current_verdict, revision_content};
use crate::read::flags::compute;
use crate::{Code, Error, Result, stat_cache};

/// `--since LAST` or `--since <event id>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Since {
    /// The latest `audit.completed`.
    Last,
    /// A named `audit.completed`.
    Event(EventId),
}

/// The arguments.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Args {
    /// Skip re-hashing files confirmed at or after this audit.
    pub since: Option<Since>,
    /// Write the report under `DIR/audit/` instead of the record's.
    pub out: Option<PathBuf>,
}

/// The counts `audit.completed` carries.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    /// Files re-hashed.
    pub files_rehashed: u64,
    /// Hash mismatches.
    pub mismatches: u64,
    /// Claim checks re-run.
    pub verdicts_rerun: u64,
    /// Verdicts that changed.
    pub verdicts_changed: u64,
    /// Open flags.
    pub open_flags: u64,
}

impl Counts {
    fn to_json(self) -> Value {
        json!({
            "files_rehashed": self.files_rehashed,
            "mismatches": self.mismatches,
            "verdicts_rerun": self.verdicts_rerun,
            "verdicts_changed": self.verdicts_changed,
            "open_flags": self.open_flags,
        })
    }
}

/// What `tm audit` produced.
#[derive(Debug, Clone)]
pub struct Report {
    /// The `audit.json` object.
    pub json: Value,
    /// Its hash, the report's name.
    pub report_hash: EventId,
    /// Where the JSON was written.
    pub json_path: PathBuf,
    /// Where the HTML was written.
    pub html_path: PathBuf,
    /// The counts.
    pub counts: Counts,
    /// The integrity findings.
    pub findings: Vec<Finding>,
    /// The `audit.completed` event.
    pub event: EventId,
}

fn finding_json(f: &Finding) -> Value {
    json!({
        "finding": f.kind.name(),
        "segment": f.segment,
        "offset": f.offset,
        "object": f.object.as_ref().map(EventId::as_str),
        "message": f.message,
    })
}

/// The audit the `--since` names: `(event_id, at)`.
fn since_audit(core: &mut Core, since: &Since) -> Result<(String, String)> {
    let mut tx = core.index.begin()?;
    let rows = match since {
        Since::Last => tx.query_rows(
            "SELECT event_id, at FROM audits ORDER BY at DESC, rowid DESC LIMIT 1",
            &[],
        )?,
        Since::Event(id) => tx.query_rows(
            "SELECT event_id, at FROM audits WHERE event_id = ?",
            &[Param::from(id.as_str())],
        )?,
    };
    tx.rollback()?;
    rows.first()
        .map(|r| {
            (
                r.text(0).unwrap_or_default().to_owned(),
                r.text(1).unwrap_or_default().to_owned(),
            )
        })
        .ok_or_else(|| {
            Error::code(
                Code::E6201,
                match since {
                    Since::Last => "no audit.completed on the chain yet".to_owned(),
                    Since::Event(id) => format!("{id} is not an audit.completed on the chain"),
                },
            )
        })
}

/// Why a re-run verdict differs from the stored one (item 80).
fn cause(stored: &Value, observed: &Map<String, Value>) -> &'static str {
    let sv = stored.get("versions");
    let ov = observed.get("versions");
    let field =
        |v: Option<&Value>, k: &str| v.and_then(|x| x.get(k)).cloned().unwrap_or(Value::Null);
    if field(sv, "duckdb") != field(ov, "duckdb") || field(sv, "build") != field(ov, "build") {
        return "DuckDB version changed";
    }
    if field(sv, "translator") != field(ov, "translator")
        || field(sv, "rules") != field(ov, "rules")
    {
        return "checker version changed";
    }
    "data changed"
}

fn outcome_summary(body: &Map<String, Value>) -> Value {
    json!({
        "evidence": body.get("evidence").and_then(|e| e.get("outcome")).cloned(),
        "assertion": body.get("assertion").and_then(|a| a.get("outcome")).cloned(),
        "final_rows_hash": body.get("final_rows_hash").cloned(),
        "key_map_hash": body.get("key_map_hash").cloned(),
    })
}

/// `tm audit [--since LAST] [--out DIR]`.
///
/// # Errors
/// `E6201` an unknown `--since`; the checks' codes; the lock's and the
/// append's; `Error::Io` when the report cannot be written.
pub fn audit(core: &mut Core, cap: &OwnerCap, args: &Args) -> Result<Report> {
    let since = match &args.since {
        Some(s) => Some(since_audit(core, s)?),
        None => None,
    };
    let mut locked = core.lock(|_, _| {})?;
    let at = locked.core.at.clone();
    let mut counts = Counts::default();

    // Integrity: chains and kept objects (item 38).
    let findings = locked.core.project.record.verify()?;

    // Files: full re-hash against the latest version's fingerprint.
    let rows = {
        let mut tx = locked.core.tx()?;
        let r = files(&mut tx)?;
        tx.rollback()?;
        r
    };
    let mut files_json = Vec::new();
    for f in &rows {
        let (latest, entry) = {
            let mut tx = locked.core.tx()?;
            let l = latest_version(&mut tx, f.identity.file)?;
            let e = stat_cache::get(&mut tx, &f.identity.path)?;
            tx.rollback()?;
            (l, e)
        };
        let Some(latest) = latest else {
            continue;
        };
        if let Some((_, since_at)) = &since
            && let Some(e) = &entry
            && e.confirmed_at
                .as_deref()
                .is_some_and(|c| c >= since_at.as_str())
        {
            continue;
        }
        let abs = locked.core.project.abs(&f.identity.path);
        if !abs.exists() {
            continue;
        }
        counts.files_rehashed += 1;
        let cand = candidate(locked.core.env(), &abs)?;
        let dependents: Vec<String> = {
            let mut tx = locked.core.tx()?;
            let r = tx.query_rows(
                "SELECT ref FROM claim_cites WHERE version = ? ORDER BY CAST(substr(ref, 2) AS INTEGER)",
                &[Param::from(latest.handle.to_string())],
            )?;
            tx.rollback()?;
            r.iter()
                .filter_map(|x| x.text(0).map(str::to_owned))
                .collect()
        };
        if cand.fingerprint == latest.fingerprint {
            let mut tx = locked.core.tx()?;
            stat_cache::put(
                &mut tx,
                &f.identity.path,
                Some(cand.snapshot),
                Some(&latest.fingerprint),
                &at,
            )?;
            tx.commit()?;
        } else {
            counts.mismatches += 1;
            files_json.push(json!({
                "path": f.identity.path,
                "version": latest.handle.to_string(),
                "expected": latest.fingerprint.as_str(),
                "observed": cand.fingerprint.as_str(),
                "last_confirmed": entry.as_ref().and_then(|e| e.confirmed_at.clone()),
                "dependents": dependents,
            }));
        }
    }

    // Verdicts: every claim's current revision re-checked and compared.
    let claims = {
        let mut tx = locked.core.tx()?;
        let c = claim_rows(&mut tx)?;
        tx.rollback()?;
        c
    };
    let limit = locked.core.settings.rows_per_page();
    let mut verdicts_json = Vec::new();
    for row in &claims {
        let (content, stored) = {
            let mut tx = locked.core.tx()?;
            let c = revision_content(&mut tx, &row.current_revision)?;
            let v = current_verdict(&mut tx, row)?;
            tx.rollback()?;
            (c, v)
        };
        let Some(stored) = stored else {
            continue;
        };
        let revision = EventId::parse(&row.current_revision)
            .ok_or_else(|| Error::code(Code::E2103, "claims.current_revision is not an ID"))?;
        let checked = check(
            locked.core,
            row.claim_ref,
            &revision,
            row.kind,
            &content,
            limit,
        )?;
        counts.verdicts_rerun += 1;
        let observed = checked.event.body();
        let stored_summary = stored
            .body
            .as_object()
            .map(outcome_summary)
            .unwrap_or(Value::Null);
        let observed_summary = outcome_summary(observed);
        if stored_summary != observed_summary {
            counts.verdicts_changed += 1;
            verdicts_json.push(json!({
                "ref": row.claim_ref.to_string(),
                "revision": row.current_revision,
                "stored": stored_summary,
                "observed": observed_summary,
                "cause": cause(&stored.body, observed),
            }));
        }
    }

    // Open items.
    let flags = compute(locked.core)?;
    let unreviewed: Vec<String> = flags
        .iter()
        .filter(|f| f.name == "promoted, not reviewed")
        .map(|f| f.subject.clone())
        .collect();
    counts.open_flags = flags.iter().filter(|f| !f.label).count() as u64;

    let since_id = since.as_ref().map(|(id, _)| id.clone());
    let report = json!({
        "audit": 1,
        "at": at,
        "since": since_id,
        "integrity": findings.iter().map(finding_json).collect::<Vec<_>>(),
        "files": files_json,
        "verdicts": verdicts_json,
        "open": {
            "flags": flags.iter().map(Flag::to_json).collect::<Vec<_>>(),
            "unreviewed": unreviewed,
        },
    });
    let report_hash = id_of(&report)?;
    let dir = args
        .out
        .clone()
        .unwrap_or_else(|| locked.core.project.record.root().to_path_buf())
        .join("audit");
    std::fs::create_dir_all(&dir).map_err(|e| Error::io(&dir, e))?;
    let stem = &report_hash.as_str()[7..];
    let json_path = dir.join(format!("{stem}.json"));
    let html_path = dir.join(format!("{stem}.html"));
    let mut text = canonical::canonical(&report)?;
    text.push('\n');
    std::fs::write(&json_path, text).map_err(|e| Error::io(&json_path, e))?;
    std::fs::write(&html_path, html(&report, &report_hash))
        .map_err(|e| Error::io(&html_path, e))?;

    let ev = KnownEvent::new(
        EventType::AuditCompleted,
        &json!({
            "report_hash": report_hash.as_str(),
            "since": since_id,
            "counts": counts.to_json(),
        }),
    )?;
    let committed = locked.commit(&owner(cap), &[ev])?;
    locked.release()?;
    Ok(Report {
        json: report,
        report_hash,
        json_path,
        html_path,
        counts,
        findings,
        event: committed.own()[0].clone(),
    })
}

/// HTML-escape a string for text and attribute positions.
#[must_use]
pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

fn cell(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => escape(s),
        other => escape(&other.to_string()),
    }
}

fn table(rows: &[Value], columns: &[&str]) -> String {
    let mut out = String::from("<table><thead><tr>");
    for c in columns {
        out.push_str(&format!("<th>{}</th>", escape(c)));
    }
    out.push_str("</tr></thead><tbody>");
    if rows.is_empty() {
        out.push_str(&format!(
            "<tr><td colspan=\"{}\">none</td></tr>",
            columns.len()
        ));
    }
    for r in rows {
        out.push_str("<tr>");
        for c in columns {
            out.push_str(&format!(
                "<td>{}</td>",
                cell(r.get(*c).unwrap_or(&Value::Null))
            ));
        }
        out.push_str("</tr>");
    }
    out.push_str("</tbody></table>");
    out
}

/// The plain `audit.html` of item 80 (P7): the report's JSON in a
/// `tm-data` block and one table per section; no external resources.
#[must_use]
pub fn html(report: &Value, report_hash: &EventId) -> String {
    let rows = |k: &str| -> Vec<Value> {
        report
            .get(k)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    };
    let open = report.get("open").cloned().unwrap_or(Value::Null);
    let flags = open
        .get("flags")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let unreviewed: Vec<Value> = open
        .get("unreviewed")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|r| json!({"ref": r}))
        .collect();
    // `</` cannot appear in a JSON script block; escape it.
    let data = report.to_string().replace("</", "<\\/");
    format!(
        "<!DOCTYPE html>\n<html lang=\"en\"><head><meta charset=\"utf-8\"><title>tm audit {hash}</title>\
<style>body{{font-family:sans-serif;margin:2em}}table{{border-collapse:collapse}}td,th{{border:1px solid #999;padding:.2em .5em;text-align:left}}</style>\
</head><body><h1>tm audit</h1><p>Report <code>{hash}</code>, at {at}{since}.</p>\
<h2>Integrity</h2>{integrity}<h2>Hash mismatches</h2>{files}<h2>Verdict changes</h2>{verdicts}\
<h2>Open flags</h2>{flags}<h2>Unreviewed claims</h2>{unreviewed}\
<script type=\"application/json\" id=\"tm-data\">{data}</script></body></html>\n",
        hash = escape(report_hash.as_str()),
        at = cell(report.get("at").unwrap_or(&Value::Null)),
        since = match report.get("since") {
            Some(Value::String(s)) => format!(", since {}", escape(s)),
            _ => String::new(),
        },
        integrity = table(
            &rows("integrity"),
            &["finding", "segment", "offset", "object", "message"]
        ),
        files = table(
            &rows("files"),
            &[
                "path",
                "version",
                "expected",
                "observed",
                "last_confirmed",
                "dependents"
            ]
        ),
        verdicts = table(
            &rows("verdicts"),
            &["ref", "revision", "stored", "observed", "cause"]
        ),
        flags = table(
            &flags,
            &["kind", "name", "subject_kind", "subject", "reason", "next"]
        ),
        unreviewed = table(&unreviewed, &["ref"]),
    )
}

/// The report path `audit.last` returns, relative to an export directory
/// (section 10).
#[must_use]
pub fn relative_html_path(report_hash: &str) -> String {
    format!(
        "audit/{}.html",
        report_hash.strip_prefix("sha256:").unwrap_or(report_hash)
    )
}

/// A path under `DIR/audit/` for a hash (the view's link target).
#[must_use]
pub fn html_path_under(dir: &Path, report_hash: &str) -> PathBuf {
    dir.join(relative_html_path(report_hash))
}
