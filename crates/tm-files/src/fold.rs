//! The Files fold (item 51; rule 6; 11.17): `file.` events into the
//! `files`, `file_versions`, `parse_specs`, and `panels` tables through an
//! [`IndexTx`]. A fold first looks the event ID up in `events` and returns
//! when present (the idempotence check of 11.17); the Core writes `events`
//! after every fold ran.

use serde_json::Value;
use tm_formats::envelope::Line;
use tm_formats::events::{EventType, KnownEvent};
use tm_formats::id::VersionHandle;
use tm_index::{IndexTx, Param};

use crate::{Code, Error, Result};

fn text(body: &KnownEvent, key: &str) -> String {
    body.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn opt_text(body: &KnownEvent, key: &str) -> Option<String> {
    body.get(key).and_then(Value::as_str).map(str::to_owned)
}

/// Fold one main-chain line. Lines of other libraries' types and opaque
/// lines are ignored.
///
/// # Errors
/// `Error::Index` when SQLite refuses; `E1002` when a `file.` line's body
/// does not validate (a chain the writer validated never produces this).
pub fn fold(tx: &mut IndexTx<'_>, line: &Line) -> Result<()> {
    let Some(ty) = line
        .envelope
        .get("type")
        .and_then(Value::as_str)
        .and_then(EventType::from_name)
    else {
        return Ok(());
    };
    if !ty.name().starts_with("file.") {
        return Ok(());
    }
    let id = line.id.as_str();
    if !tx
        .query_rows("SELECT 1 FROM events WHERE id = ?", &[Param::from(id)])?
        .is_empty()
    {
        return Ok(());
    }
    let tv = line.envelope.get("tv").and_then(Value::as_u64).unwrap_or(1);
    let body = KnownEvent::new_at(ty, tv, line.envelope.get("body").unwrap_or(&Value::Null))?;
    match ty {
        EventType::FileRegistered => registered(tx, id, &body),
        EventType::FileVersionAdded => version_added(tx, id, &body),
        EventType::FileParseSpecSet => parse_spec_set(tx, id, &body),
        EventType::FileKept => kept(tx, &body),
        EventType::FilePanelsSet => panels_set(tx, &body),
        EventType::FileMarkedRemovable => update(
            tx,
            "UPDATE files SET removable = 1 WHERE file = ?",
            &[Param::from(text(&body, "file"))],
        ),
        EventType::FileRestored => update(
            tx,
            "UPDATE files SET retired = 0 WHERE file = ?",
            &[Param::from(text(&body, "file"))],
        ),
        EventType::FileRenamed => update(
            tx,
            "UPDATE files SET path = ? WHERE file = ?",
            &[
                Param::from(text(&body, "to")),
                Param::from(text(&body, "file")),
            ],
        ),
        _ => Ok(()),
    }
}

fn update(tx: &mut IndexTx<'_>, sql: &str, params: &[Param]) -> Result<()> {
    tx.execute(sql, params)?;
    Ok(())
}

fn registered(tx: &mut IndexTx<'_>, id: &str, body: &KnownEvent) -> Result<()> {
    update(
        tx,
        "INSERT INTO files (file, kind, path, title, description, source, level, retired, removable, agent_denied, registered_event) \
         VALUES (?, ?, ?, ?, ?, ?, 'registered', 0, 0, 0, ?) \
         ON CONFLICT(file) DO UPDATE SET title = excluded.title, description = excluded.description, \
         source = excluded.source, level = 'registered', registered_event = excluded.registered_event",
        &[
            Param::from(text(body, "file")),
            Param::from(text(body, "kind")),
            Param::from(text(body, "path")),
            Param::from(text(body, "title")),
            Param::from(text(body, "description")),
            Param::from(opt_text(body, "source")),
            Param::from(id),
        ],
    )
}

fn version_added(tx: &mut IndexTx<'_>, id: &str, body: &KnownEvent) -> Result<()> {
    let version = text(body, "version");
    let n = VersionHandle::parse(&version)
        .map(|h| h.version)
        .ok_or_else(|| Error::code(Code::E1002, "file.version_added: version handle"))?;
    let origin = body.get("origin").cloned().unwrap_or(Value::Null);
    update(
        tx,
        "INSERT OR IGNORE INTO file_versions (version, file, n, fingerprint, origin_kind, origin_run, content_label, size, kept_object, fingerprint_only, decompressed_fingerprint, event_id) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, NULL, 0, ?, ?)",
        &[
            Param::from(version),
            Param::from(text(body, "file")),
            Param::from(n),
            Param::from(text(body, "fingerprint")),
            Param::from(
                origin
                    .get("kind")
                    .and_then(Value::as_str)
                    .unwrap_or("registered"),
            ),
            Param::from(origin.get("run").and_then(Value::as_str).map(str::to_owned)),
            Param::from(opt_text(body, "content_label")),
            Param::from(body.get("size").and_then(Value::as_u64).unwrap_or(0)),
            Param::from(opt_text(body, "decompressed_fingerprint")),
            Param::from(id),
        ],
    )
}

fn parse_spec_set(tx: &mut IndexTx<'_>, id: &str, body: &KnownEvent) -> Result<()> {
    let spec = body.get("spec").cloned().unwrap_or(Value::Null);
    let canonical = tm_formats::canonical::canonical(&spec)?;
    update(
        tx,
        "INSERT OR REPLACE INTO parse_specs (file, spec, spec_fingerprint, event_id) VALUES (?, ?, ?, ?)",
        &[
            Param::from(text(body, "file")),
            Param::from(canonical),
            Param::from(text(body, "spec_fingerprint")),
            Param::from(id),
        ],
    )
}

fn kept(tx: &mut IndexTx<'_>, body: &KnownEvent) -> Result<()> {
    update(
        tx,
        "UPDATE file_versions SET kept_object = ?, fingerprint_only = ? WHERE version = ?",
        &[
            Param::from(text(body, "object")),
            Param::from(
                body.get("fingerprint_only")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            ),
            Param::from(text(body, "version")),
        ],
    )
}

fn panels_set(tx: &mut IndexTx<'_>, body: &KnownEvent) -> Result<()> {
    let version = text(body, "version");
    update(
        tx,
        "DELETE FROM panels WHERE version = ?",
        &[Param::from(version.as_str())],
    )?;
    for name in body
        .get("panels")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
    {
        update(
            tx,
            "INSERT OR IGNORE INTO panels (version, name) VALUES (?, ?)",
            &[
                Param::from(version.as_str()),
                Param::from(name.as_str().unwrap_or_default()),
            ],
        )?;
    }
    Ok(())
}
