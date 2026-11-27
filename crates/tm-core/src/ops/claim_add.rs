//! `claim.add` (section 9; CUJ 6; items 64 and 74): allocate `C<n>` under
//! the lock, write the claim straight into the main chain, run both checks,
//! store the verdict, and rescan the report for the ref's spans.

use std::path::{Path, PathBuf};

use tm_claims::claim::{Claim, Content, Kind};
use tm_claims::refs;
use tm_formats::canonical;
use tm_formats::citation::Citation;
use tm_formats::events::{Event, KnownEvent};
use tm_formats::id::{ClaimRef, EventId, VersionHandle};
use tm_owner_cap::OwnerCap;

use crate::ops::claim_check::{Checked, check, require_citable};
use crate::ops::report_check::rescan;
use crate::ops::{Core, Locked, owner, report_by_handle_or_path, spec_of};
use crate::read::Flag;
use crate::read::flags::flags_of_claim;
use crate::{Code, Error, Result, index};

/// `tm claim add`'s arguments (item 83's spelling, parsed by the CLI).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Args {
    /// `--report PATH|HANDLE`.
    pub report: String,
    /// `--ref REF`: the author's expectation of the allocated ref.
    pub expected_ref: Option<ClaimRef>,
    /// `--sentence S`.
    pub sentence: String,
    /// `--cited HANDLE...` in the 11.23 CLI spelling.
    pub cited: Vec<String>,
    /// `--steps FILE`.
    pub steps_file: Option<PathBuf>,
    /// `--assert FILE`.
    pub assert_file: Option<PathBuf>,
    /// `--kind`.
    pub kind: Kind,
    /// `--qualifiers Q`.
    pub qualifiers: Option<String>,
}

/// What `claim.add` did.
#[derive(Debug, Clone)]
pub struct Added {
    /// The allocated ref.
    pub claim_ref: ClaimRef,
    /// The `claim.added` event.
    pub added: EventId,
    /// The `claim.checked` event.
    pub checked_event: EventId,
    /// The check: verdict, per-step tables, evidence.
    pub check: Checked,
    /// The claim's flags after the add.
    pub flags: Vec<Flag>,
    /// Spans of the ref found in the report.
    pub spans: usize,
}

/// Read a `--steps` or `--assert` file as I-JSON (11.1).
///
/// # Errors
/// `E1003` not I-JSON; `E1001` a float; `Error::Io`.
pub fn read_json_file(path: &Path) -> Result<serde_json::Value> {
    let bytes = crate::read_file(path)?;
    Ok(canonical::parse_bytes(&bytes)?)
}

/// The version a CLI citation spelling names: the text up to its first
/// unescaped `#`.
///
/// # Errors
/// `E6007` when it is not a version handle.
pub fn cited_version(text: &str) -> Result<VersionHandle> {
    let mut end = text.len();
    let b = text.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'\\' => i += 2,
            b'#' => {
                end = i;
                break;
            }
            _ => i += 1,
        }
    }
    VersionHandle::parse(&text[..end]).ok_or_else(|| {
        Error::code(
            Code::E6007,
            format!("{text:?} does not start with a version handle F<n>@v<m>"),
        )
    })
}

/// Build the content a `claim.added` or a supersede's replacement records
/// (11.16): citations parsed per the cited versions' specs, the `--steps`
/// and `--assert` files read, `cited` the versions the citations and the
/// steps name, each once.
///
/// # Errors
/// `E6007` malformed citation; `E1003`, `E1001`, `E1004` for the files;
/// `E6002` for a kind that does not match its inputs; `Error::Index`.
pub fn build_content(core: &mut Core, args: &Args) -> Result<Content> {
    let steps = args.steps_file.as_deref().map(read_json_file).transpose()?;
    let assertion = args
        .assert_file
        .as_deref()
        .map(read_json_file)
        .transpose()?;
    match (
        args.kind.has_assertion_check(),
        assertion.is_some(),
        steps.is_some(),
    ) {
        (true, false, _) => {
            return Err(Error::code(
                Code::E6002,
                format!(
                    "a {} claim carries an assertion (`--assert`)",
                    args.kind.as_str()
                ),
            ));
        }
        (false, true, _) | (false, _, true) => {
            return Err(Error::code(
                Code::E6002,
                "a qualitative claim carries neither steps nor an assertion",
            ));
        }
        (true, true, _) => {}
        (false, false, false) => {}
    }
    let mut citations = Vec::new();
    let mut cited: Vec<VersionHandle> = Vec::new();
    let mut tx = core.tx()?;
    for text in &args.cited {
        let v = cited_version(text)?;
        let spec = spec_of(&mut tx, v.file)?.map(|(s, _)| s);
        citations.push(Citation::parse_cli(text, spec.as_ref())?);
        if !cited.contains(&v) {
            cited.push(v);
        }
    }
    tx.rollback()?;
    if let Some(s) = &steps
        && let Some(tables) = s.get("tables").and_then(serde_json::Value::as_object)
    {
        for h in tables.values() {
            if let Some(v) = h.as_str().and_then(VersionHandle::parse)
                && !cited.contains(&v)
            {
                cited.push(v);
            }
        }
    }
    Ok(Content {
        sentence: args.sentence.clone(),
        cited,
        citations,
        steps,
        assertion,
        qualifiers: args.qualifiers.clone(),
    })
}

/// `tm claim add`'s refusals that need no lock, in section 9's order:
/// citability, the `cells` keys at add time (`E6003`), the steps and
/// assertion against the cited specs (the first 11.8 or 11.9 error), and a
/// `page` citation on a file without a page tree (`E3302`).
///
/// # Errors
/// `E6001`, `E6002`, `E6003`, `E6004`, `E6005`, `E6006`, `E3302`.
pub fn validate(core: &mut Core, content: &Content) -> Result<()> {
    require_citable(core, content)?;
    // E6003: a cells citation's key absent from the pinned version now.
    for c in &content.citations {
        match c {
            Citation::Cells { version, keys, .. } => {
                let (env, index) = super::split(core);
                let mut tx = index.begin()?;
                let (file, row) = super::citable(&mut tx, version)?;
                let (batch, spec, _) = super::table_of(env, &mut tx, &file.identity, &row)?;
                tx.rollback()?;
                let present: Vec<String> = tm_files::parse::key_tuples(&batch, &spec)?
                    .iter()
                    .map(|t| tm_files::parse::key_tuple_json(t, &(0..t.len()).collect::<Vec<_>>()))
                    .collect();
                for k in keys {
                    let text =
                        tm_files::parse::key_tuple_json(k, &(0..k.len()).collect::<Vec<_>>());
                    if !present.contains(&text) {
                        return Err(Error::code(
                            Code::E6003,
                            format!("{version} has no row with key {text}"),
                        ));
                    }
                }
            }
            Citation::Page { version, .. } => {
                let mut tx = core.tx()?;
                let (file, row) = super::citable(&mut tx, version)?;
                tx.rollback()?;
                let scratch = core.project.parse_cache_dir().join("scratch");
                let path = super::version_file(core.env(), &file.identity, &row, &scratch)?;
                tm_files::pdf::page_count(&path)?;
            }
            _ => {}
        }
    }
    if let Some(assertion) = &content.assertion {
        // The 11.8 and 11.9 checks against the cited specs, before anything
        // runs; a refusal, not a verdict (section 9).
        let mut schemas = std::collections::BTreeMap::new();
        let mut cited_schema = None;
        let mut tx = core.tx()?;
        if let Some(steps) = &content.steps
            && let Some(tables) = steps.get("tables").and_then(serde_json::Value::as_object)
        {
            for (name, h) in tables {
                if let Some(v) = h.as_str().and_then(VersionHandle::parse)
                    && let Some((spec, _)) = spec_of(&mut tx, v.file)?
                {
                    schemas.insert(
                        name.clone(),
                        tm_formats::steps::TableSchema::from_spec(&spec),
                    );
                }
            }
        }
        if content.steps.is_none()
            && let Some(v) = content.citations.iter().find_map(|c| match c {
                Citation::Cells { version, .. } => Some(*version),
                _ => None,
            })
            && let Some((spec, _)) = spec_of(&mut tx, v.file)?
        {
            cited_schema = Some(tm_formats::steps::TableSchema::from_spec(&spec));
        }
        tx.rollback()?;
        tm_checker::rules::validate(
            content.steps.as_ref(),
            assertion,
            &schemas,
            cited_schema.as_ref(),
        )?;
    }
    Ok(())
}

/// The chain's last `claim.added` with a `C<n>` ref (item 64).
fn chain_last_owner_ref(core: &Core) -> Result<Option<ClaimRef>> {
    let mut last = None;
    for e in core.project.record.events_of_type("claim.added")? {
        let e = e?;
        if let Event::Known(k) = &e.line.event
            && let Some(c) = Claim::from_body(k)
            && matches!(c.claim_ref, ClaimRef::Owner(_))
        {
            last = Some(c.claim_ref);
        }
    }
    Ok(last)
}

/// Allocate the next `C<n>` under the lock (item 64): the index's highest
/// confirmed against the chain's last `claim.added`; a disagreement
/// rebuilds the index and retries once, then is `E2103`.
///
/// # Errors
/// `E2103`; `Error::Index`.
pub fn allocate(locked: &mut Locked<'_>) -> Result<ClaimRef> {
    let chain_last = chain_last_owner_ref(locked.core)?;
    for attempt in 0..2 {
        let highest = {
            let mut tx = locked.core.tx()?;
            let h = refs::highest_owner_ref(&mut tx)?;
            tx.rollback()?;
            h
        };
        match refs::allocate(highest, chain_last) {
            Ok(r) => return Ok(r),
            Err(m) if attempt == 0 => {
                index::rebuild(&mut locked.core.index, &locked.core.project.record)?;
                let _ = m;
            }
            Err(m) => {
                return Err(Error::code(
                    Code::E2103,
                    format!("{m} after a rebuild of the index"),
                ));
            }
        }
    }
    Err(Error::code(Code::E2103, "ref allocation did not settle"))
}

/// The locked half of an add: allocate, check the expectation, write
/// `claim.added` and `claim.checked` (and `extra` after them), rescan the
/// report. Shared with `claim.supersede`.
///
/// # Errors
/// `E6201` when `expected` is not the allocated ref; the check's codes;
/// the append's.
pub fn add_locked(
    locked: &mut Locked<'_>,
    cap: &OwnerCap,
    expected: Option<ClaimRef>,
    kind: Kind,
    report: tm_formats::id::ReportHandle,
    supersedes: Option<ClaimRef>,
    content: Content,
    extra: impl FnOnce(ClaimRef) -> Result<Vec<KnownEvent>>,
) -> Result<Added> {
    let claim_ref = allocate(locked)?;
    if let Some(e) = expected
        && e != claim_ref
    {
        return Err(Error::code(
            Code::E6201,
            format!("--ref {e} is not the next owner ref, which is {claim_ref}"),
        ));
    }
    let claim = Claim {
        claim_ref,
        kind,
        report,
        supersedes,
        content,
    };
    let added = claim.added_event()?;
    let actor = owner(cap);
    let ids = locked.preview_ids(&actor, std::slice::from_ref(&added))?;
    let revision = ids[0].clone();
    let limit = locked.core.settings.rows_per_page();
    let checked = check(
        locked.core,
        claim_ref,
        &revision,
        kind,
        &claim.content,
        limit,
    )?;
    let mut events = vec![added, checked.event.clone()];
    events.extend(extra(claim_ref)?);
    let committed = locked.commit(&actor, &events)?;
    let own = committed.own();
    let spans = rescan(locked.core)?
        .iter()
        .filter(|s| s.claim_ref == claim_ref)
        .count();
    let flags = flags_of_claim(locked.core, claim_ref)?;
    Ok(Added {
        claim_ref,
        added: own[0].clone(),
        checked_event: own[1].clone(),
        check: checked,
        flags,
        spans,
    })
}

/// `tm claim add`.
///
/// # Errors
/// Section 9's `claim.add` codes; the lock's and the append's.
pub fn add(core: &mut Core, cap: &OwnerCap, args: &Args) -> Result<Added> {
    let report = report_by_handle_or_path(core, &args.report)?;
    let content = build_content(core, args)?;
    validate(core, &content)?;
    let mut locked = core.lock(|_, _| {})?;
    let added = add_locked(
        &mut locked,
        cap,
        args.expected_ref,
        args.kind,
        report.report,
        None,
        content,
        |_| Ok(Vec::new()),
    )?;
    locked.release()?;
    Ok(added)
}
