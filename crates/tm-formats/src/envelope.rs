//! The event envelope and its line form (design 11.2 and 11.11, item 13).
//!
//! A line is the canonical envelope without `id`, with `,"id":"<id>"`
//! inserted before its final `}`. [`read_line`] checks the line form and the
//! ID, then the envelope, then (for a known type and `tv`) the body, and
//! returns a [`Line`]. [`write_line`] builds a line for a [`KnownEvent`];
//! an opaque event has no write path.
//!
//! Every envelope error is `E1002` with `field` at the offending member;
//! JSON-level errors keep their own codes (`E1001`, `E1003`).

use serde_json::{Map, Value};

use crate::canonical;
use crate::errors::{Code, Error, ptr};
use crate::events::{Event, EventType, KnownEvent, OpaqueEvent, is_at};
use crate::id::{EventId, id_of, is_sha256, positive};
use crate::version;

/// `actor.kind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActorKind {
    /// `owner`
    Owner,
    /// `session`
    Session,
    /// `viewer`
    Viewer,
}

impl ActorKind {
    fn as_str(self) -> &'static str {
        match self {
            ActorKind::Owner => "owner",
            ActorKind::Session => "session",
            ActorKind::Viewer => "viewer",
        }
    }
}

/// `actor`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Actor {
    /// Kind.
    pub kind: ActorKind,
    /// Non-empty identifier.
    pub id: String,
}

/// `src.source`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    /// `cli`
    Cli,
    /// `static-view`
    StaticView,
    /// `viewer`
    Viewer,
    /// `wrapper`
    Wrapper,
}

impl SourceKind {
    fn as_str(self) -> &'static str {
        match self {
            SourceKind::Cli => "cli",
            SourceKind::StaticView => "static-view",
            SourceKind::Viewer => "viewer",
            SourceKind::Wrapper => "wrapper",
        }
    }
}

/// `src`: the D23 facts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::struct_field_names, reason = "the fields mirror the 11.2 keys")]
pub struct Source {
    /// Where the action came from.
    pub source: SourceKind,
    /// Standard input was a terminal.
    pub tty: bool,
    /// A known harness variable was set.
    pub harness_env: bool,
}

/// Everything in an envelope except `type`, `tv`, and `body`: what a writer
/// supplies alongside a [`KnownEvent`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Head {
    /// `main`, `S<n>`, or `V<n>`.
    pub chain: String,
    /// From 1 per chain.
    pub seq: u64,
    /// The previous event's ID, or the chain's genesis string.
    pub prev: EventId,
    /// `YYYY-MM-DDTHH:MM:SSZ`.
    pub at: String,
    /// Who.
    pub actor: Actor,
    /// D23 facts.
    pub src: Source,
    /// Batch number (main chain only).
    pub batch: Option<u64>,
    /// Lock epoch (main chain only).
    pub epoch: Option<u64>,
}

/// A line that passed [`read_line`].
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    /// The event ID.
    pub id: EventId,
    /// The envelope without `id`, as read.
    pub envelope: Value,
    /// The interpreted or opaque event.
    pub event: Event,
}

fn e(at: &str, msg: impl Into<String>) -> Error {
    Error::at(Code::E1002, at, msg)
}

/// True for `main`, `S<n>`, or `V<n>`.
#[must_use]
pub fn is_chain(s: &str) -> bool {
    s == "main"
        || s.strip_prefix(['S', 'V'])
            .is_some_and(|n| positive(n).is_some())
}

fn is_type_name(s: &str) -> bool {
    let Some((ns, name)) = s.split_once('.') else {
        return false;
    };
    !ns.is_empty()
        && ns.bytes().all(|b| b.is_ascii_lowercase())
        && name.bytes().next().is_some_and(|b| b.is_ascii_lowercase())
        && name.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')
}

fn int_at_least(m: &Map<String, Value>, key: &str, min: u64) -> Result<u64, Error> {
    m[key].as_u64().filter(|n| *n >= min).ok_or_else(|| {
        e(
            &ptr("", key),
            format!("`{key}` is an integer of at least {min}"),
        )
    })
}

/// The text of a line for an envelope without `id`, and its ID.
///
/// # Errors
/// `E1001` if the envelope cannot be canonicalized.
pub fn line_of(envelope: &Value) -> Result<(String, EventId), Error> {
    let mut text = canonical::canonical(envelope)?;
    let id = id_of(envelope)?;
    if text.pop() != Some('}') {
        return Err(e("", "an envelope is an object"));
    }
    text.push_str(",\"id\":\"");
    text.push_str(id.as_str());
    text.push_str("\"}");
    Ok((text, id))
}

/// Read one line (without its trailing newline).
///
/// # Errors
/// `E1003` or `E1001` if the line is not acceptable JSON; `E1002` with
/// `field` at the offending member for any envelope or body problem, `/id`
/// for a line form or ID mismatch.
pub fn read_line(line: &str) -> Result<Line, Error> {
    let v = canonical::parse(line)?;
    let Value::Object(mut m) = v else {
        return Err(e("", "an envelope is an object"));
    };
    let Some(Value::String(id)) = m.shift_remove("id") else {
        return Err(e("/id", "an envelope line ends with its `id`"));
    };
    let envelope = Value::Object(m);
    let (expected, expected_id) = line_of(&envelope)?;
    if id != expected_id.as_str() {
        return Err(e("/id", "`id` does not match the envelope"));
    }
    if line != expected {
        return Err(e(
            "/id",
            "the line is not the canonical envelope with `id` appended",
        ));
    }
    let event = check_envelope(&envelope)?;
    Ok(Line {
        id: expected_id,
        envelope,
        event,
    })
}

const BASE_KEYS: &[&str] = &[
    "env", "type", "tv", "chain", "seq", "prev", "at", "actor", "src", "body",
];
const RESERVED_KEYS: &[&str] = &["sig", "collab"];

/// Validate an envelope (without `id`) and its body.
///
/// # Errors
/// `E1002` with `field` at the offending member.
pub fn check_envelope(envelope: &Value) -> Result<Event, Error> {
    let Some(m) = envelope.as_object() else {
        return Err(e("", "an envelope is an object"));
    };
    if !m.contains_key("env") {
        return Err(e("/env", "required member `env` is absent"));
    }
    let env = int_at_least(m, "env", 1)?;
    if env > version::ENVELOPE {
        return Ok(Event::Opaque(OpaqueEvent {
            ty: String::new(),
            tv: 0,
            envelope: envelope.clone(),
        }));
    }
    for k in BASE_KEYS {
        if !m.contains_key(*k) {
            return Err(e(&ptr("", k), format!("required member `{k}` is absent")));
        }
    }
    let chain = m["chain"]
        .as_str()
        .filter(|c| is_chain(c))
        .ok_or_else(|| e("/chain", "`chain` is main, S<n>, or V<n>"))?;
    let main = chain == "main";
    for k in ["batch", "epoch"] {
        if main && !m.contains_key(k) {
            return Err(e(&ptr("", k), format!("a main-chain event carries `{k}`")));
        }
    }
    let mut extra: Vec<&String> = m
        .keys()
        .filter(|k| {
            let k = k.as_str();
            !BASE_KEYS.contains(&k)
                && !RESERVED_KEYS.contains(&k)
                && !(main && (k == "batch" || k == "epoch"))
        })
        .collect();
    extra.sort_by(|a, b| canonical::utf16_cmp(a, b));
    if let Some(k) = extra.first() {
        return Err(e(&ptr("", k), format!("member `{k}` is not defined here")));
    }
    let ty = m["type"]
        .as_str()
        .filter(|t| is_type_name(t))
        .ok_or_else(|| e("/type", "`type` is <namespace>.<name>"))?;
    let tv = int_at_least(m, "tv", 1)?;
    int_at_least(m, "seq", 1)?;
    if !m["prev"].as_str().is_some_and(is_sha256) {
        return Err(e("/prev", "`prev` is sha256: and 64 hex digits"));
    }
    if !m["at"].as_str().is_some_and(is_at) {
        return Err(e("/at", "`at` is YYYY-MM-DDTHH:MM:SSZ"));
    }
    read_actor(&m["actor"])?;
    read_source(&m["src"])?;
    if !m["body"].is_object() {
        return Err(e("/body", "`body` is an object"));
    }
    if main {
        int_at_least(m, "batch", 0)?;
        int_at_least(m, "epoch", 0)?;
    }
    let Some(known) = EventType::from_name(ty) else {
        return Ok(Event::Opaque(OpaqueEvent {
            ty: ty.to_owned(),
            tv,
            envelope: envelope.clone(),
        }));
    };
    match version::read_policy(&[known.tv()], tv) {
        version::Read::Known => Ok(Event::Known(KnownEvent::new(known, &m["body"])?)),
        version::Read::Opaque => Ok(Event::Opaque(OpaqueEvent {
            ty: ty.to_owned(),
            tv,
            envelope: envelope.clone(),
        })),
        version::Read::Invalid => Err(e("/tv", format!("unknown version {tv} of {ty}"))),
    }
}

fn read_actor(v: &Value) -> Result<Actor, Error> {
    let m = v
        .as_object()
        .ok_or_else(|| e("/actor", "`actor` is an object"))?;
    crate::events::check_object(
        m,
        &[
            (
                "kind",
                crate::events::Form::Choice(&["owner", "session", "viewer"]),
            ),
            ("id", crate::events::Form::NonEmptyStr),
        ],
        "/actor",
    )?;
    let kind = match m["kind"].as_str() {
        Some("owner") => ActorKind::Owner,
        Some("session") => ActorKind::Session,
        _ => ActorKind::Viewer,
    };
    Ok(Actor {
        kind,
        id: m["id"].as_str().unwrap_or_default().to_owned(),
    })
}

fn read_source(v: &Value) -> Result<Source, Error> {
    use crate::events::Form;
    let m = v
        .as_object()
        .ok_or_else(|| e("/src", "`src` is an object"))?;
    crate::events::check_object(
        m,
        &[
            (
                "source",
                Form::Choice(&["cli", "static-view", "viewer", "wrapper"]),
            ),
            ("tty", Form::Any),
            ("harness_env", Form::Any),
        ],
        "/src",
    )?;
    let flag = |k: &str| {
        m[k].as_bool()
            .ok_or_else(|| e(&ptr("/src", k), format!("`{k}` is a boolean")))
    };
    let source = match m["source"].as_str() {
        Some("cli") => SourceKind::Cli,
        Some("static-view") => SourceKind::StaticView,
        Some("viewer") => SourceKind::Viewer,
        _ => SourceKind::Wrapper,
    };
    Ok(Source {
        source,
        tty: flag("tty")?,
        harness_env: flag("harness_env")?,
    })
}

/// Build the line for a known event. The writer policy (11.11) holds by
/// construction: a [`KnownEvent`] is always at its type's known `tv`, and
/// an opaque event cannot be passed here.
///
/// # Errors
/// `E1002` if the head is invalid (for example `batch` on a journal).
pub fn write_line(head: &Head, event: &KnownEvent) -> Result<(String, EventId), Error> {
    let mut m = Map::new();
    m.insert("env".into(), version::ENVELOPE.into());
    m.insert("type".into(), event.ty().name().into());
    m.insert("tv".into(), event.ty().tv().into());
    m.insert("chain".into(), head.chain.clone().into());
    m.insert("seq".into(), head.seq.into());
    m.insert("prev".into(), head.prev.as_str().into());
    m.insert("at".into(), head.at.clone().into());
    let mut actor = Map::new();
    actor.insert("kind".into(), head.actor.kind.as_str().into());
    actor.insert("id".into(), head.actor.id.clone().into());
    m.insert("actor".into(), Value::Object(actor));
    let mut src = Map::new();
    src.insert("source".into(), head.src.source.as_str().into());
    src.insert("tty".into(), head.src.tty.into());
    src.insert("harness_env".into(), head.src.harness_env.into());
    m.insert("src".into(), Value::Object(src));
    if let Some(b) = head.batch {
        m.insert("batch".into(), b.into());
    }
    if let Some(ep) = head.epoch {
        m.insert("epoch".into(), ep.into());
    }
    m.insert("body".into(), Value::Object(event.body().clone()));
    let envelope = Value::Object(m);
    check_envelope(&envelope)?;
    line_of(&envelope)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::genesis;
    use serde_json::json;

    fn head() -> Head {
        Head {
            chain: "main".into(),
            seq: 1,
            prev: genesis("main", "p1"),
            at: "2026-11-09T14:02:11Z".into(),
            actor: Actor {
                kind: ActorKind::Owner,
                id: "o1".into(),
            },
            src: Source {
                source: SourceKind::Cli,
                tty: true,
                harness_env: false,
            },
            batch: Some(1),
            epoch: Some(1),
        }
    }

    #[test]
    fn write_then_read() {
        let ev = KnownEvent::new(EventType::FileRestored, &json!({"file":"F7"})).unwrap();
        let (line, id) = write_line(&head(), &ev).unwrap();
        let read = read_line(&line).unwrap();
        assert_eq!(read.id, id);
        assert_eq!(read.event, Event::Known(ev));
        // One changed byte is a mismatch at /id.
        let tampered = line.replace("F7", "F8");
        assert_eq!(
            read_line(&tampered).unwrap_err().field.as_deref(),
            Some("/id")
        );
    }

    #[test]
    fn opaque_and_invalid() {
        let mut env = json!({"env":1,"type":"run.started","tv":1,"chain":"main","seq":1,"prev":genesis("main","p").as_str(),"at":"2026-11-09T14:02:11Z","actor":{"kind":"owner","id":"o"},"src":{"source":"cli","tty":false,"harness_env":false},"body":{"x":1},"batch":0,"epoch":0});
        assert!(matches!(check_envelope(&env).unwrap(), Event::Opaque(_)));
        env["type"] = json!("file.restored");
        env["tv"] = json!(2);
        assert!(matches!(check_envelope(&env).unwrap(), Event::Opaque(_)));
        env["tv"] = json!(1);
        let err = check_envelope(&env).unwrap_err();
        assert_eq!(
            (err.code, err.field.unwrap()),
            (Code::E1002, "/body/file".to_owned())
        );
        env["chain"] = json!("S4");
        let err = check_envelope(&env).unwrap_err();
        assert_eq!(err.field.unwrap(), "/batch");
    }
}
