//! File identities (item 44; 6.1): an input's identity is its
//! repository-relative path; the output identity type (analysis, pattern,
//! relative path) is declared for gate C and unused in gate A.

use serde_json::{Value, json};
use tm_formats::events::{EventType, KnownEvent};
use tm_formats::id::FileHandle;
use tm_formats::sort_order::nfc;

use crate::attributes::{Action, State};
use crate::{Code, Error, Result, event};

/// 6.1 Kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    /// A project path, registered by the owner.
    Input,
    /// A declared output of an analysis (gate C).
    Output(OutputIdentity),
}

impl Kind {
    /// The 11.16 spelling.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Kind::Input => "input",
            Kind::Output(_) => "output",
        }
    }
}

/// An output identity (6.1): the analysis, the declared pattern, and the
/// path relative to it. Declared here for gate C; no gate A operation
/// creates one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputIdentity {
    /// The analysis handle.
    pub analysis: String,
    /// The declared output pattern.
    pub pattern: String,
    /// The path relative to the pattern.
    pub relative: String,
}

/// 6.1 Level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// Declared input or output, no title; not citable.
    Tracked,
    /// Title and description present; citable.
    Registered,
}

impl Level {
    /// The 11.17 `files.level` spelling.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Level::Tracked => "tracked",
            Level::Registered => "registered",
        }
    }
}

/// What `file.register` supplies (section 9's row: title and description
/// are required, `E3002` otherwise).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Registration {
    /// Non-empty.
    pub title: String,
    /// Non-empty.
    pub description: String,
    /// Provider or origin text, if any.
    pub source: Option<String>,
}

/// One file identity with its 6.1 attributes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    /// `F<n>`.
    pub file: FileHandle,
    /// Input or output.
    pub kind: Kind,
    /// The repository-relative path (NFC, `/`-separated).
    pub path: String,
    /// Title, once registered.
    pub title: Option<String>,
    /// Description, once registered.
    pub description: Option<String>,
    /// Source, once registered.
    pub source: Option<String>,
    /// Tracked or registered.
    pub level: Level,
    /// Retired (gate C).
    pub retired: bool,
    /// Marked removable by the owner.
    pub removable: bool,
    /// On the agent-denied setting (gate B).
    pub agent_denied: bool,
}

/// Check and normalize an input path (item 44): repository-relative,
/// `/`-separated, NFC, non-empty, with no empty, `.`, or `..` component.
///
/// # Errors
/// `E3001` with `field` `null` for any other form.
pub fn check_input_path(path: &str) -> Result<String> {
    let bad = |why: &str| Error::code(Code::E3001, format!("path {path:?}: {why}"));
    if path.is_empty() {
        return Err(bad("empty"));
    }
    if path.starts_with('/') {
        return Err(bad("absolute; an input path is repository-relative"));
    }
    if path.contains('\\') {
        return Err(bad("contains a backslash; components are separated by `/`"));
    }
    for component in path.split('/') {
        match component {
            "" => return Err(bad("has an empty component")),
            "." | ".." => return Err(bad("has a `.` or `..` component")),
            _ => {}
        }
    }
    Ok(nfc(path))
}

impl Identity {
    /// Register an input at `path` (section 9 `file.register`): a new
    /// identity at level `registered`, with its `file.registered` event.
    ///
    /// # Errors
    /// `E3001` for a path of the wrong form; `E3002` for an empty title or
    /// description.
    pub fn register(
        file: FileHandle,
        path: &str,
        reg: &Registration,
    ) -> Result<(Identity, KnownEvent)> {
        let path = check_input_path(path)?;
        if reg.title.trim().is_empty() {
            return Err(Error::code(Code::E3002, "a registered file has a title"));
        }
        if reg.description.trim().is_empty() {
            return Err(Error::code(
                Code::E3002,
                "a registered file has a description",
            ));
        }
        let identity = Identity {
            file,
            kind: Kind::Input,
            path: path.clone(),
            title: Some(reg.title.clone()),
            description: Some(reg.description.clone()),
            source: reg.source.clone(),
            level: Level::Registered,
            retired: false,
            removable: false,
            agent_denied: false,
        };
        let ev = event(
            EventType::FileRegistered,
            &json!({
                "file": file.to_string(),
                "kind": "input",
                "path": path,
                "title": reg.title,
                "description": reg.description,
                "source": reg.source,
            }),
        )?;
        Ok((identity, ev))
    }

    /// Register a tracked identity (gate C: a declared path gains a title),
    /// through the 6.3 transition.
    ///
    /// # Errors
    /// `E3201` when the identity is retired; `E3002` as [`Identity::register`].
    pub fn register_tracked(&self, reg: &Registration) -> Result<(Identity, KnownEvent)> {
        let next = self.state().apply(Action::Register)?;
        let (mut id, ev) = Identity::register(self.file, &self.path, reg)?;
        id.kind = self.kind.clone();
        id.removable = next.removable;
        id.agent_denied = self.agent_denied;
        Ok((id, ev))
    }

    /// Rename an input (6.3: an explicit command that keeps versions).
    ///
    /// # Errors
    /// `E3201` for an output or a retired identity; `E3001` for a path of
    /// the wrong form.
    pub fn rename(&self, to: &str) -> Result<(Identity, KnownEvent)> {
        self.state().apply(Action::Rename)?;
        let to = check_input_path(to)?;
        let mut id = self.clone();
        id.path.clone_from(&to);
        let ev = event(
            EventType::FileRenamed,
            &json!({"file": self.file.to_string(), "from": self.path, "to": to}),
        )?;
        Ok((id, ev))
    }

    /// Mark removable (6.1).
    ///
    /// # Errors
    /// `E3201` when already removable.
    pub fn mark_removable(&self) -> Result<(Identity, KnownEvent)> {
        let next = self.state().apply(Action::MarkRemovable)?;
        let mut id = self.clone();
        id.removable = next.removable;
        let ev = event(
            EventType::FileMarkedRemovable,
            &json!({"file": self.file.to_string()}),
        )?;
        Ok((id, ev))
    }

    /// Restore a retired identity (`tm file restore`).
    ///
    /// # Errors
    /// `E3201` when the identity is not retired.
    pub fn restore(&self) -> Result<(Identity, KnownEvent)> {
        let next = self.state().apply(Action::Restore)?;
        let mut id = self.clone();
        id.retired = next.retired;
        let ev = event(
            EventType::FileRestored,
            &json!({"file": self.file.to_string()}),
        )?;
        Ok((id, ev))
    }

    /// The identity-level part of the 6.3 state (the version part is the
    /// caller's, see [`State`]).
    #[must_use]
    pub fn state(&self) -> State {
        State {
            level: self.level,
            is_input: matches!(self.kind, Kind::Input),
            retired: self.retired,
            removable: self.removable,
            latest: None,
        }
    }

    /// The `files` row values the fold writes, for callers that build state
    /// from an event (`file.registered` body).
    ///
    /// # Errors
    /// `E1002` when `body` is not a `file.registered` body.
    pub fn from_registered(body: &Value) -> Result<Identity> {
        let ev = event(EventType::FileRegistered, body)?;
        let s = |k: &str| {
            ev.get(k)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        let file = FileHandle::parse(&s("file"))
            .ok_or_else(|| Error::code(Code::E1002, "file.registered carries a file handle"))?;
        Ok(Identity {
            file,
            kind: if s("kind") == "input" {
                Kind::Input
            } else {
                Kind::Output(OutputIdentity {
                    analysis: String::new(),
                    pattern: String::new(),
                    relative: s("path"),
                })
            },
            path: s("path"),
            title: Some(s("title")),
            description: Some(s("description")),
            source: ev.get("source").and_then(Value::as_str).map(str::to_owned),
            level: Level::Registered,
            retired: false,
            removable: false,
            agent_denied: false,
        })
    }
}
