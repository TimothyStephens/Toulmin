//! `table.spec` (section 9; 11.7; item 47; item 74): complete the owner's
//! draft spec against the file, check the key, write the parse cache, and
//! record `file.parse_spec_set`.
//!
//! The file parsed is the latest version's bytes: the file on disk when it
//! still matches that version's fingerprint, else the kept object; a file
//! on disk that differs from the latest version is `E3203` with the advice
//! to run `tm status`, which versions it (item 74, P7).

use serde_json::{Value, json};
use tm_formats::events::{EventType, KnownEvent};
use tm_formats::id::EventId;
use tm_formats::parse_spec::ParseSpec;
use tm_owner_cap::OwnerCap;

use crate::ops::{Core, file_by_handle_or_path, latest_version, owner, version_file};
use crate::{Code, Error, Result};

/// What `table.spec` did.
#[derive(Debug, Clone, PartialEq)]
pub struct SpecSet {
    /// The completed spec.
    pub spec: ParseSpec,
    /// Its fingerprint.
    pub spec_fingerprint: EventId,
    /// Rows parsed.
    pub rows: u64,
    /// The event ID.
    pub event: EventId,
}

/// `tm table spec PATH|HANDLE` with the draft spec as JSON (the CLI builds
/// it from `--key`, `--types`, `--missing`, ...).
///
/// # Errors
/// `E3101` duplicate key; `E3102` type parse failure; `E3103` unknown
/// column; `E3104` unsupported encoding; `E3105` invalid parse spec;
/// `E6001` not registered or no version; `E3203` bytes not available; the
/// lock's and the append's.
pub fn set(
    core: &mut Core,
    cap: &OwnerCap,
    handle_or_path: &str,
    draft: &Value,
) -> Result<SpecSet> {
    let row = file_by_handle_or_path(core, handle_or_path)?;
    let identity = row.identity.clone();
    let latest = {
        let mut tx = core.tx()?;
        let l = latest_version(&mut tx, identity.file)?;
        tx.rollback()?;
        l
    }
    .ok_or_else(|| {
        Error::code(
            Code::E6001,
            format!("{} has no version yet (`tm file hash`)", identity.path),
        )
    })?;
    if identity.level != tm_files::Level::Registered {
        return Err(Error::code(
            Code::E6001,
            format!("{} is not registered", identity.path),
        ));
    }
    let scratch = core.project.parse_cache_dir().join("scratch");
    let path = version_file(core.env(), &identity, &latest, &scratch).map_err(|e| match e {
        Error::Format(f) if f.code == Code::E3203 => Error::code(
            Code::E3203,
            format!(
                "{} on disk differs from {} and its bytes are not kept; run `tm status` to version the file first",
                identity.path, latest.handle
            ),
        ),
        other => other,
    })?;
    let table = tm_files::parse::complete_spec(&path, draft, &core.engine_settings())?;
    let fp = table.spec.fingerprint();
    core.parse_cache()
        .store(&latest.fingerprint, &fp, &table.batch)?;
    let ev = KnownEvent::new(
        EventType::FileParseSpecSet,
        &json!({
            "file": identity.file.to_string(),
            "spec": table.spec.to_json(),
            "spec_fingerprint": fp.as_str(),
        }),
    )?;
    let mut locked = core.lock(|_, _| {})?;
    let committed = locked.commit(&owner(cap), &[ev])?;
    locked.release()?;
    Ok(SpecSet {
        spec: table.spec,
        spec_fingerprint: fp,
        rows: table.batch.num_rows() as u64,
        event: committed.own()[0].clone(),
    })
}
