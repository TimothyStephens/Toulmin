//! `report.check` (section 9; 11.17; 11.22; item 74): rescan every
//! registered report from the working tree with the 11.22 scanner and
//! rewrite the scan-derived `spans` table; drift per span against the
//! claim's current sentence. `tm report add` and `tm status` run the same
//! rescan.

use serde_json::{Value, json};
use tm_claims::drift::{Drift, drift};
use tm_claims::spans::scan;
use tm_formats::id::{ClaimRef, ReportHandle};
use tm_index::Param;

use crate::ops::{Core, reports};
use crate::read::claims::{claim_row, revision_content};
use crate::{Code, Error, Result};

/// One scanned span as the `spans` table holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpanRow {
    /// The report.
    pub report: ReportHandle,
    /// The ref.
    pub claim_ref: ClaimRef,
    /// Line, from 1.
    pub line: u64,
    /// Ordinal among the line's spans, from 0 (11.17).
    pub pos: u64,
    /// The recorded text.
    pub text: String,
    /// Holds inline code (drift is not checkable).
    pub inline_code: bool,
    /// The nearest heading's class.
    pub section_kind: Option<String>,
    /// The other attributes as written.
    pub attributes: Vec<String>,
}

impl SpanRow {
    /// The subject a span flag names: `T<n>:<line>:<pos>`.
    #[must_use]
    pub fn subject(&self) -> String {
        format!("{}:{}:{}", self.report, self.line, self.pos)
    }
}

/// Scan one registered report's file now.
///
/// # Errors
/// `Error::Io` when the file does not read.
pub fn spans_of_report(core: &Core, rel: &str) -> Result<Vec<tm_claims::Span>> {
    let text = crate::read_text(&core.project.abs(rel))?;
    Ok(scan(&text))
}

fn rows_of(report: ReportHandle, spans: &[tm_claims::Span]) -> Vec<SpanRow> {
    let mut out = Vec::new();
    let mut line = 0;
    let mut pos = 0;
    for s in spans {
        if s.line != line {
            line = s.line;
            pos = 0;
        } else {
            pos += 1;
        }
        out.push(SpanRow {
            report,
            claim_ref: s.claim_ref,
            line: s.line,
            pos,
            text: s.text.clone(),
            inline_code: s.inline_code,
            section_kind: s.section_kind.clone(),
            attributes: s.attributes.clone(),
        });
    }
    out
}

/// Rescan every registered report and rewrite `spans` (11.17). A report
/// whose file is missing keeps no rows. Returns every span.
///
/// # Errors
/// `Error::Index`; `Error::Io` for a report file that exists but does not
/// read.
pub fn rescan(core: &mut Core) -> Result<Vec<SpanRow>> {
    let list = {
        let mut tx = core.tx()?;
        let r = reports(&mut tx)?;
        tx.rollback()?;
        r
    };
    let mut all = Vec::new();
    for r in list {
        let path = core.project.abs(&r.path);
        if !path.is_file() {
            continue;
        }
        let spans = spans_of_report(core, &r.path)?;
        all.extend(rows_of(r.report, &spans));
    }
    let mut tx = core.index.begin()?;
    tx.execute("DELETE FROM spans", &[])?;
    for s in &all {
        let attributes = tm_formats::canonical::canonical(&Value::Array(
            s.attributes.iter().cloned().map(Value::String).collect(),
        ))?;
        tx.execute(
            "INSERT OR REPLACE INTO spans (report, ref, line, pos, sentence, inline_code, section_kind, attributes) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            &[
                Param::from(s.report.to_string()),
                Param::from(s.claim_ref.to_string()),
                Param::from(s.line),
                Param::from(s.pos),
                Param::from(s.text.as_str()),
                Param::from(s.inline_code),
                Param::from(s.section_kind.clone()),
                Param::from(attributes),
            ],
        )?;
    }
    tx.commit()?;
    Ok(all)
}

/// The `spans` rows as stored, in file order.
///
/// # Errors
/// `Error::Index`.
pub fn stored_spans(
    tx: &mut tm_index::IndexTx<'_>,
    report: Option<ReportHandle>,
) -> Result<Vec<SpanRow>> {
    let (sql, params): (&str, Vec<Param>) = match report {
        Some(r) => (
            "SELECT report, ref, line, pos, sentence, inline_code, section_kind, attributes FROM spans WHERE report = ? ORDER BY line, pos",
            vec![Param::from(r.to_string())],
        ),
        None => (
            "SELECT report, ref, line, pos, sentence, inline_code, section_kind, attributes FROM spans ORDER BY report, line, pos",
            Vec::new(),
        ),
    };
    let rows = tx.query_rows(sql, &params)?;
    Ok(rows
        .iter()
        .filter_map(|r| {
            Some(SpanRow {
                report: ReportHandle::parse(r.text(0)?)?,
                claim_ref: ClaimRef::parse(r.text(1)?)?,
                line: r.get(2).as_u64()?,
                pos: r.get(3).as_u64().unwrap_or(0),
                text: r.text(4)?.to_owned(),
                inline_code: r.int(5).unwrap_or(0) != 0,
                section_kind: r.text(6).map(str::to_owned),
                attributes: r
                    .text(7)
                    .and_then(|t| serde_json::from_str::<Value>(t).ok())
                    .and_then(|v| {
                        v.as_array().map(|a| {
                            a.iter()
                                .filter_map(|x| x.as_str().map(str::to_owned))
                                .collect()
                        })
                    })
                    .unwrap_or_default(),
            })
        })
        .collect())
}

/// One span's drift result (`report.check`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpanDrift {
    /// The span.
    pub span: SpanRow,
    /// The claim's current sentence, `None` for an unknown ref.
    pub sentence: Option<String>,
    /// The drift, `None` for an unknown ref.
    pub drift: Option<Drift>,
    /// The claim's state, `None` for an unknown ref.
    pub state: Option<tm_claims::State>,
}

impl SpanDrift {
    /// The JSON shape (`reports.spans`).
    #[must_use]
    pub fn to_json(&self) -> Value {
        json!({
            "report": self.span.report.to_string(),
            "ref": self.span.claim_ref.to_string(),
            "line": self.span.line,
            "pos": self.span.pos,
            "text": self.span.text,
            "inline_code": self.span.inline_code,
            "section_kind": self.span.section_kind,
            "attributes": self.span.attributes,
            "sentence": self.sentence,
            "drift": self.drift.map(|d| match d {
                Drift::Same => "same",
                Drift::Drifted => "drifted",
                Drift::NotCheckable => "not_checkable",
            }),
            "state": self.state.map(tm_claims::State::as_str),
        })
    }
}

/// Drift per stored span of `report` (all reports for `None`), against
/// each claim's current sentence (11.22).
///
/// # Errors
/// `Error::Index`.
pub fn drifts(core: &mut Core, report: Option<ReportHandle>) -> Result<Vec<SpanDrift>> {
    let mut tx = core.index.begin()?;
    let spans = stored_spans(&mut tx, report)?;
    let mut out = Vec::new();
    for s in spans {
        let row = claim_row(&mut tx, s.claim_ref)?;
        let (sentence, state) = match &row {
            Some(r) => (
                Some(revision_content(&mut tx, &r.current_revision)?.sentence),
                Some(r.state),
            ),
            None => (None, None),
        };
        let span = tm_claims::Span {
            claim_ref: s.claim_ref,
            line: s.line,
            text: s.text.clone(),
            inline_code: s.inline_code,
            section_kind: s.section_kind.clone(),
            attributes: s.attributes.clone(),
        };
        let d = sentence.as_deref().map(|sent| drift(sent, &span));
        out.push(SpanDrift {
            span: s,
            sentence,
            drift: d,
            state,
        });
    }
    tx.rollback()?;
    Ok(out)
}

/// `tm report check [REPORT]`: rescan, then drift per span.
///
/// # Errors
/// `E6201` unknown report; `Error::Index`; `Error::Io`.
pub fn check(core: &mut Core, report: Option<&str>) -> Result<Vec<SpanDrift>> {
    let handle = match report {
        Some(text) => Some(super::report_by_handle_or_path(core, text)?.report),
        None => None,
    };
    rescan(core)?;
    if let Some(h) = handle {
        let mut tx = core.tx()?;
        let known = reports(&mut tx)?.iter().any(|r| r.report == h);
        tx.rollback()?;
        if !known {
            return Err(Error::code(Code::E6201, format!("no report {h}")));
        }
    }
    drifts(core, handle)
}
