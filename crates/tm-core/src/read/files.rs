//! `files.list`, `files.show`, `files.lineage`, `files.rows`, `files.bytes`
//! (section 10; item 78).

use serde_json::{Value, json};
use tm_files::Level;
use tm_formats::id::VersionHandle;
use tm_formats::typed_value::TypedValue;
use tm_index::{IndexTx, Param};

use crate::ops::{
    Core, FileRow, VersionRow, file_by_handle_or_path, files, panels_of, spec_of, split, table_of,
    version_by_handle, version_bytes, versions_of,
};
use crate::read::flags::compute;
use crate::read::{Flag, list};
use crate::{Code, Error, Result};

/// `files.list`'s filters (section 10); each narrows the list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListFilter {
    /// `tracked` or `registered`.
    pub level: Option<Level>,
    /// `input` or `output`.
    pub kind: Option<String>,
    /// Cited by at least one claim (`true`) or by none (`false`).
    pub cited: Option<bool>,
    /// Retired or not.
    pub retired: Option<bool>,
    /// An analysis handle (gate C; matches nothing in gate A).
    pub analysis: Option<String>,
    /// `LIKE` over title, description, and path.
    pub text: Option<String>,
}

fn cited_count(tx: &mut IndexTx<'_>, file: &str) -> Result<u64> {
    let rows = tx.query_rows(
        "SELECT COUNT(*) FROM claim_cites JOIN file_versions USING (version) WHERE file = ?",
        &[Param::from(file)],
    )?;
    Ok(rows.first().and_then(|r| r.get(0).as_u64()).unwrap_or(0))
}

fn version_json(v: &VersionRow) -> Value {
    json!({
        "version": v.handle.to_string(),
        "fingerprint": v.fingerprint.as_str(),
        "origin": {"kind": v.origin_kind, "run": v.origin_run},
        "content_label": v.content_label,
        "size": v.size,
        "kept": match v.kept() {
            tm_files::Kept::No => "no",
            tm_files::Kept::Bytes => "bytes",
            tm_files::Kept::FingerprintOnly => "fingerprint_only",
        },
        "kept_object": v.kept_object,
        "decompressed_fingerprint": v.decompressed_fingerprint,
        "event": v.event_id,
    })
}

fn identity_json(f: &FileRow, latest: Option<&VersionRow>, cited: u64, has_spec: bool) -> Value {
    json!({
        "file": f.identity.file.to_string(),
        "kind": f.identity.kind.as_str(),
        "path": f.identity.path,
        "title": f.identity.title,
        "description": f.identity.description,
        "source": f.identity.source,
        "latest_version": latest.map(|v| v.handle.to_string()),
        "attributes": {
            "level": f.identity.level.as_str(),
            "retired": f.identity.retired,
            "restored": f.restored,
            "removable": f.identity.removable,
            "agent_denied": f.identity.agent_denied,
            "parse_spec": has_spec,
        },
        "cited_count": cited,
    })
}

/// `files.list`.
///
/// # Errors
/// `Error::Index`.
pub fn files_list(core: &mut Core, filter: &ListFilter) -> Result<Value> {
    let all_flags = compute(core)?;
    let mut tx = core.index.begin()?;
    let mut out = Vec::new();
    for f in files(&mut tx)? {
        if filter.level.is_some_and(|l| l != f.identity.level)
            || filter
                .kind
                .as_deref()
                .is_some_and(|k| k != f.identity.kind.as_str())
            || filter.retired.is_some_and(|r| r != f.identity.retired)
            || filter.analysis.is_some()
        {
            continue;
        }
        if let Some(t) = &filter.text {
            let needle = t.to_lowercase();
            let hay = format!(
                "{} {} {}",
                f.identity.title.clone().unwrap_or_default(),
                f.identity.description.clone().unwrap_or_default(),
                f.identity.path
            )
            .to_lowercase();
            if !hay.contains(&needle) {
                continue;
            }
        }
        let file = f.identity.file.to_string();
        let cited = cited_count(&mut tx, &file)?;
        if filter.cited.is_some_and(|c| c != (cited > 0)) {
            continue;
        }
        let versions = versions_of(&mut tx, f.identity.file)?;
        let has_spec = spec_of(&mut tx, f.identity.file)?.is_some();
        let mut j = identity_json(&f, versions.last(), cited, has_spec);
        let subjects: Vec<String> = std::iter::once(file.clone())
            .chain(versions.iter().map(|v| v.handle.to_string()))
            .collect();
        j["flags"] = list(
            all_flags
                .iter()
                .filter(|fl| {
                    (fl.subject_kind == "file" || fl.subject_kind == "version")
                        && subjects.contains(&fl.subject)
                })
                .map(Flag::to_json),
        );
        out.push(j);
    }
    tx.rollback()?;
    Ok(Value::Array(out))
}

/// `files.show`: identity, every version, attributes, labels and flags,
/// parse spec, panel labels.
///
/// # Errors
/// `E6001` unknown file; `Error::Index`.
pub fn files_show(core: &mut Core, handle_or_path: &str) -> Result<Value> {
    let f = file_by_handle_or_path(core, handle_or_path)?;
    let all_flags = compute(core)?;
    let mut tx = core.index.begin()?;
    let file = f.identity.file.to_string();
    let versions = versions_of(&mut tx, f.identity.file)?;
    let cited = cited_count(&mut tx, &file)?;
    let spec = spec_of(&mut tx, f.identity.file)?;
    let mut j = identity_json(&f, versions.last(), cited, spec.is_some());
    let mut vs = Vec::new();
    for v in &versions {
        let mut vj = version_json(v);
        vj["panels"] = json!(panels_of(&mut tx, &v.handle)?);
        vj["cited_by"] = json!(
            tx.query_rows(
                "SELECT ref FROM claim_cites WHERE version = ? ORDER BY CAST(substr(ref, 2) AS INTEGER)",
                &[Param::from(v.handle.to_string())],
            )?
            .iter()
            .filter_map(|r| r.text(0).map(str::to_owned))
            .collect::<Vec<_>>()
        );
        vs.push(vj);
    }
    tx.rollback()?;
    j["versions"] = Value::Array(vs);
    j["parse_spec"] = spec.as_ref().map_or(
        Value::Null,
        |(s, fp)| json!({"spec": s.to_json(), "fingerprint": fp.as_str()}),
    );
    let subjects: Vec<String> = std::iter::once(file)
        .chain(versions.iter().map(|v| v.handle.to_string()))
        .collect();
    j["flags"] = list(
        all_flags
            .iter()
            .filter(|fl| {
                (fl.subject_kind == "file" || fl.subject_kind == "version")
                    && subjects.contains(&fl.subject)
            })
            .map(Flag::to_json),
    );
    Ok(j)
}

/// `files.lineage`: the tree from a version back to registered inputs. In
/// gate A there are no runs, so the tree is the registered input itself as
/// the only node (the gate A exit criterion's tree stub).
///
/// # Errors
/// `E6001` unknown version; `Error::Index`.
pub fn files_lineage(
    core: &mut Core,
    version: &VersionHandle,
    depth: Option<u64>,
) -> Result<Value> {
    let f = file_by_handle_or_path(core, &version.file.to_string())?;
    let mut tx = core.index.begin()?;
    let v = version_by_handle(&mut tx, version)?
        .ok_or_else(|| Error::code(Code::E6001, format!("{version} does not exist")))?;
    tx.rollback()?;
    Ok(json!({
        "version": version.to_string(),
        "depth": depth,
        "nodes": [{
            "id": version.to_string(),
            "kind": "registered_input",
            "file": f.identity.file.to_string(),
            "path": f.identity.path,
            "title": f.identity.title,
            "fingerprint": v.fingerprint.as_str(),
            "labels": Vec::<String>::new(),
        }],
        "edges": Vec::<Value>::new(),
    }))
}

/// `files.rows`' row selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowSelect {
    /// Rows whose key tuple is among these.
    Keys(Vec<Vec<TypedValue>>),
    /// A 0-based range `[start, end)` in key order.
    Range(u64, u64),
}

/// `files.rows`: rows from the parse cache, by key values or by range, with
/// the listed columns (all when `None`), each row with its 0-based position
/// so neighbors can be fetched.
///
/// # Errors
/// `E6001` unknown version or no parse spec; `E6005` an unknown column;
/// the parser's codes; `Error::Index`.
pub fn files_rows(
    core: &mut Core,
    version: &VersionHandle,
    select: &RowSelect,
    columns: Option<&[String]>,
) -> Result<Value> {
    let f = file_by_handle_or_path(core, &version.file.to_string())?;
    let (env, index) = split(core);
    let mut tx = index.begin()?;
    let v = version_by_handle(&mut tx, version)?
        .ok_or_else(|| Error::code(Code::E6001, format!("{version} does not exist")))?;
    let (batch, spec, _) = table_of(env, &mut tx, &f.identity, &v)?;
    tx.rollback()?;
    let rows = tm_files::parse::typed_rows(&batch, &spec)?;
    let keys = tm_files::parse::key_indexes(&spec);
    let names: Vec<String> = spec.columns.iter().map(|c| c.name.clone()).collect();
    let wanted: Vec<usize> = match columns {
        None => (0..names.len()).collect(),
        Some(cols) => {
            cols.iter()
                .map(|c| {
                    names.iter().position(|n| n == c).ok_or_else(|| {
                        Error::at(Code::E6005, "/columns", format!("no column {c:?}"))
                    })
                })
                .collect::<Result<_>>()?
        }
    };
    // Key order, as the Checker's step 0 has it (11.18.2 Initial order).
    let mut order: Vec<usize> = (0..rows.len()).collect();
    order.sort_by(|&a, &b| {
        for &k in &keys {
            let o = rows[a][k]
                .compare(&rows[b][k])
                .unwrap_or(std::cmp::Ordering::Equal);
            if o != std::cmp::Ordering::Equal {
                return o;
            }
        }
        std::cmp::Ordering::Equal
    });
    let selected: Vec<(usize, &Vec<TypedValue>)> = match select {
        RowSelect::Range(start, end) => order
            .iter()
            .enumerate()
            .skip(usize::try_from(*start).unwrap_or(usize::MAX))
            .take(usize::try_from(end.saturating_sub(*start)).unwrap_or(0))
            .map(|(pos, &i)| (pos, &rows[i]))
            .collect(),
        RowSelect::Keys(wanted_keys) => {
            let want: Vec<String> = wanted_keys
                .iter()
                .map(|k| tm_files::parse::key_tuple_json(k, &(0..k.len()).collect::<Vec<_>>()))
                .collect();
            order
                .iter()
                .enumerate()
                .filter(|&(_, &i)| want.contains(&tm_files::parse::key_tuple_json(&rows[i], &keys)))
                .map(|(pos, &i)| (pos, &rows[i]))
                .collect()
        }
    };
    Ok(json!({
        "version": version.to_string(),
        "columns": wanted.iter().map(|&i| json!({"name": names[i], "type": spec.columns[i].ty.as_str()})).collect::<Vec<_>>(),
        "key": spec.key,
        "row_count": rows.len(),
        "rows": selected.iter().map(|(pos, r)| json!({
            "position": pos,
            "values": wanted.iter().map(|&i| r[i].to_json()).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    }))
}

/// `files.bytes` (owner only): kept or on-disk bytes.
///
/// # Errors
/// `E6001` unknown version; `E3203` bytes not available.
pub fn files_bytes(core: &mut Core, version: &VersionHandle) -> Result<Vec<u8>> {
    let f = file_by_handle_or_path(core, &version.file.to_string())?;
    let mut tx = core.index.begin()?;
    let v = version_by_handle(&mut tx, version)?
        .ok_or_else(|| Error::code(Code::E6001, format!("{version} does not exist")))?;
    tx.rollback()?;
    version_bytes(core.env(), &f.identity, &v)
}
