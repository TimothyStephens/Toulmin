//! The two system checks on a claim (section 7; 7.3; item 74's
//! `claim_check.rs`): evidence exists, from Files facts the Core gathers
//! and the Claims library's 7.3 table, and the assertion, from the Checker
//! over the cited tables Files hands over as Arrow. The result is the
//! `claim.checked` event (11.16). `tm claim check REF` re-checks a claim's
//! current revision.

use std::collections::BTreeMap;

use serde_json::{Map, Value, json};
use tm_checker::{InputTable, Request, RunResult, run};
use tm_claims::claim::{Content, Kind, against, checked_event};
use tm_claims::evidence_exists::{
    self, Base, CitationType, Evidence, Situation, VersionFacts, subject,
};
use tm_formats::citation::Citation;
use tm_formats::events::KnownEvent;
use tm_formats::id::{ClaimRef, EventId, VersionHandle};
use tm_formats::typed_value::TypedValue;
use tm_owner_cap::OwnerCap;

use crate::ops::{
    Core, Env, FileRow, VersionRow, citable, file_by_handle, owner, panels_of, split, table_of,
    version_by_handle, version_file,
};
use crate::read::claims::{claim_row, revision_content};
use crate::{Code, Error, Result};

/// A check's result before it is written.
#[derive(Debug, Clone)]
pub struct Checked {
    /// The `claim.checked` event.
    pub event: KnownEvent,
    /// The Checker's result, when it ran (its per-step tables for the
    /// claim page).
    pub run: Option<RunResult>,
    /// The evidence outcome.
    pub evidence: Evidence,
}

/// What the Core knows about one cited version at check time.
struct Cited {
    file: FileRow,
    row: VersionRow,
}

fn cited_versions(core: &mut Core, content: &Content) -> Result<BTreeMap<String, Cited>> {
    let mut out = BTreeMap::new();
    let mut tx = core.tx()?;
    for v in content
        .cited
        .iter()
        .chain(content.citations.iter().filter_map(|c| match c {
            Citation::Cells { version, .. }
            | Citation::WholeFile { version }
            | Citation::Page { version, .. }
            | Citation::Panel { version, .. } => Some(version),
            Citation::Reserved(_) => None,
        }))
    {
        if out.contains_key(&v.to_string()) {
            continue;
        }
        let file = file_by_handle(&mut tx, v.file)?
            .ok_or_else(|| Error::code(Code::E6001, format!("{v}: no file {}", v.file)))?;
        let row = version_by_handle(&mut tx, v)?
            .ok_or_else(|| Error::code(Code::E6001, format!("{v} does not exist")))?;
        out.insert(v.to_string(), Cited { file, row });
    }
    tx.rollback()?;
    Ok(out)
}

/// The 7.3 column facts for a version: on disk and matching now, kept,
/// fingerprint only, removable.
fn facts(env: Env<'_>, c: &Cited) -> VersionFacts {
    let path = env.project.abs(&c.file.identity.path);
    let on_disk = path.is_file() || path.is_dir();
    let matches = on_disk
        && super::file_register::candidate(env, &path)
            .is_ok_and(|cand| cand.fingerprint == c.row.fingerprint);
    VersionFacts {
        on_disk,
        matches,
        bytes_kept: c.row.bytes_kept(),
        fingerprint_only: c.row.fingerprint_only,
        removable: c.file.identity.removable,
    }
}

fn tuple_json(t: &[TypedValue]) -> String {
    tm_formats::canonical::canonical(&Value::Array(t.iter().map(TypedValue::to_json).collect()))
        .unwrap_or_default()
}

/// The pinned test of one citation (7.3's first column): every cited key
/// present, the page within the page count, the panel registered.
fn pinned_test(core: &mut Core, c: &Citation, cv: &Cited) -> Result<bool> {
    match c {
        Citation::Cells { keys, .. } => {
            let (env, index) = split(core);
            let mut tx = index.begin()?;
            let (batch, spec, _) = table_of(env, &mut tx, &cv.file.identity, &cv.row)?;
            tx.rollback()?;
            let present: Vec<String> = tm_files::parse::key_tuples(&batch, &spec)?
                .iter()
                .map(|t| tuple_json(t))
                .collect();
            Ok(keys.iter().all(|k| present.contains(&tuple_json(k))))
        }
        Citation::Page { page, .. } => {
            let scratch = core.project.parse_cache_dir().join("scratch");
            let path = version_file(core.env(), &cv.file.identity, &cv.row, &scratch)?;
            Ok(tm_files::pdf::page_count(&path)? >= *page)
        }
        Citation::Panel { panel, .. } => {
            let mut tx = core.tx()?;
            let names = panels_of(&mut tx, &cv.row.handle)?;
            tx.rollback()?;
            Ok(names.contains(panel))
        }
        Citation::WholeFile { .. } | Citation::Reserved(_) => Ok(true),
    }
}

/// The evidence-exists check over every citation (7.3). The pinned test
/// runs only where the cell needs it (`Base::Check`).
///
/// # Errors
/// `Error::Index`; the parser's codes when a `cells` test needs the table
/// and it does not read.
fn evidence(
    core: &mut Core,
    content: &Content,
    cited: &BTreeMap<String, Cited>,
) -> Result<Evidence> {
    let mut results = Vec::new();
    for c in &content.citations {
        let t = CitationType::of(c);
        let version = match c {
            Citation::Cells { version, .. }
            | Citation::WholeFile { version }
            | Citation::Page { version, .. }
            | Citation::Panel { version, .. } => Some(*version),
            Citation::Reserved(_) => None,
        };
        let Some(v) = version else {
            let r = evidence_exists::evaluate(t, Situation::MissingNotKept, false, || false);
            results.push((subject(c), r));
            continue;
        };
        let Some(cv) = cited.get(&v.to_string()) else {
            continue;
        };
        let situation = Situation::of(facts(core.env(), cv));
        let retired = cv.file.identity.retired;
        let needs_test = evidence_exists::cell(t, situation, retired).base == Base::Check;
        let passed = if needs_test {
            pinned_test(core, c, cv)?
        } else {
            false
        };
        let r = evidence_exists::evaluate(t, situation, retired, || passed);
        results.push((subject(c), r));
    }
    Ok(Evidence::combine(&results))
}

/// Run the Checker for a claim that has an assertion: the cited tables as
/// Arrow by their steps name (or by handle with `steps` null), the steps,
/// the assertion, the citations.
///
/// # Errors
/// `E6001` when a cited table has no parse spec; the parser's codes;
/// `E3203` when a table's bytes are not available.
pub fn checker_run(core: &mut Core, content: &Content, limit: usize) -> Result<Option<RunResult>> {
    let Some(assertion) = &content.assertion else {
        return Ok(None);
    };
    let cited = cited_versions(core, content)?;
    let mut tables: BTreeMap<String, InputTable> = BTreeMap::new();
    let (env, index) = split(core);
    let mut tx = index.begin()?;
    match &content.steps {
        Some(steps) => {
            let entries = steps
                .get("tables")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            for (name, handle) in entries {
                let h = handle
                    .as_str()
                    .and_then(VersionHandle::parse)
                    .ok_or_else(|| {
                        Error::at(
                            Code::E6004,
                            format!("/tables/{name}"),
                            format!("{handle} is not a version handle"),
                        )
                    })?;
                let c = cited.get(&h.to_string()).ok_or_else(|| {
                    Error::code(Code::E6001, format!("{h} is not among the cited versions"))
                })?;
                let (batch, spec, _) = table_of(env, &mut tx, &c.file.identity, &c.row)?;
                tables.insert(
                    name,
                    InputTable {
                        handle: h.to_string(),
                        spec,
                        batch,
                    },
                );
            }
        }
        None => {
            for c in &content.citations {
                if let Citation::Cells { version, .. } = c
                    && let Some(cv) = cited.get(&version.to_string())
                {
                    let (batch, spec, _) = table_of(env, &mut tx, &cv.file.identity, &cv.row)?;
                    tables.insert(
                        version.to_string(),
                        InputTable {
                            handle: version.to_string(),
                            spec,
                            batch,
                        },
                    );
                }
            }
        }
    }
    tx.rollback()?;
    let settings = env.engine_settings();
    let req = Request {
        tables: &tables,
        steps: content.steps.as_ref(),
        assertion,
        citations: &content.citations,
        settings: &settings,
        limit,
    };
    Ok(Some(run(&req)))
}

/// The `versions` block of a claim the Checker does not run (11.16): the
/// translator and rule versions, the pinned engine's version as
/// `tm-duckdb` reports it, and the build flavour.
///
/// # Errors
/// `Error::Engine` when the engine does not open.
pub fn not_run_versions(core: &Core) -> Result<Value> {
    let facts = tm_duckdb::open(core.engine_settings())?.facts()?;
    Ok(json!({
        "parse_specs": {},
        "translator": tm_checker::translate::TRANSLATOR_VERSION,
        "duckdb": facts.version,
        "extensions": Vec::<String>::new(),
        "rules": tm_checker::rules::RULES_VERSION,
        "build": facts.build,
    }))
}

/// The Checker's members of a `claim.checked` body for a claim the Checker
/// does not run (11.16).
///
/// # Errors
/// As [`not_run_versions`].
pub fn not_run_members(core: &Core) -> Result<Map<String, Value>> {
    let mut m = Map::new();
    m.insert("assertion".into(), Value::Null);
    m.insert("steps".into(), Value::Array(Vec::new()));
    m.insert("final_rows_hash".into(), Value::Null);
    m.insert("key_map_hash".into(), Value::Null);
    m.insert("key_map".into(), Value::Null);
    m.insert("sql".into(), Value::String(String::new()));
    m.insert("versions".into(), not_run_versions(core)?);
    m.insert("error".into(), Value::Null);
    Ok(m)
}

/// The version the key map belongs to (11.16): the `from` table's with
/// `steps`, else the `cells` citation's, else none.
#[must_use]
pub fn key_map_version(content: &Content) -> Option<VersionHandle> {
    if let Some(steps) = &content.steps {
        let from = steps.get("from").and_then(Value::as_str)?;
        return steps
            .get("tables")
            .and_then(|t| t.get(from))
            .and_then(Value::as_str)
            .and_then(VersionHandle::parse);
    }
    content.citations.iter().find_map(|c| match c {
        Citation::Cells { version, .. } => Some(*version),
        _ => None,
    })
}

/// Run both checks on a revision and build its `claim.checked`.
///
/// # Errors
/// The checks' codes above; `E1002` when the body does not validate.
pub fn check(
    core: &mut Core,
    claim_ref: ClaimRef,
    revision: &EventId,
    kind: Kind,
    content: &Content,
    limit: usize,
) -> Result<Checked> {
    let cited = cited_versions(core, content)?;
    let evidence = evidence(core, content, &cited)?;
    let run = if kind.has_assertion_check() {
        checker_run(core, content, limit)?
    } else {
        None
    };
    let members = match &run {
        Some(r) => r.verdict.body_members(),
        None => not_run_members(core)?,
    };
    let ag = against(key_map_version(content), &content.cited);
    let event = checked_event(claim_ref, revision, &ag, &evidence, &members)?;
    Ok(Checked {
        event,
        run,
        evidence,
    })
}

/// Every cited version is citable (6.3): `tm claim add`'s permission.
///
/// # Errors
/// `E6001`.
pub fn require_citable(core: &mut Core, content: &Content) -> Result<()> {
    let mut tx = core.tx()?;
    for v in &content.cited {
        citable(&mut tx, v)?;
    }
    tx.rollback()?;
    Ok(())
}

/// What `tm claim check REF` returns.
#[derive(Debug, Clone)]
pub struct Rechecked {
    /// The check.
    pub checked: Checked,
    /// The `claim.checked` event's ID.
    pub event: EventId,
}

/// `tm claim check REF`: re-check the current revision (section 9's
/// `claim.check` row).
///
/// # Errors
/// `E6201` unknown ref; the checks' codes; the lock's and the append's.
pub fn recheck(core: &mut Core, cap: &OwnerCap, claim_ref: ClaimRef) -> Result<Rechecked> {
    let (row, content) = {
        let mut tx = core.tx()?;
        let row = claim_row(&mut tx, claim_ref)?
            .ok_or_else(|| Error::code(Code::E6201, format!("no claim {claim_ref}")))?;
        let content = revision_content(&mut tx, &row.current_revision)?;
        tx.rollback()?;
        (row, content)
    };
    let limit = core.settings.rows_per_page();
    let revision = EventId::parse(&row.current_revision)
        .ok_or_else(|| Error::code(Code::E2103, "claims.current_revision is not an ID"))?;
    let checked = check(core, claim_ref, &revision, row.kind, &content, limit)?;
    let mut locked = core.lock(|_, _| {})?;
    let committed = locked.commit(&owner(cap), &[checked.event.clone()])?;
    locked.release()?;
    Ok(Rechecked {
        event: committed.own()[0].clone(),
        checked,
    })
}

/// The fingerprint of a registered report's file on disk now (a review
/// records it, 5.4).
///
/// # Errors
/// `Error::Io` when the file does not read.
pub fn report_fingerprint(core: &Core, rel: &str) -> Result<EventId> {
    let path = core.project.abs(rel);
    Ok(tm_fsfacts::hash::hash_file(&path, &core.settings.hash_settings())?.fingerprint)
}
