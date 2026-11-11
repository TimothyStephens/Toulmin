//! The error catalog and error values (design 11.12, item 20).
//!
//! The catalog lives in `catalog.rs` as data; `build.rs` generates the
//! [`Code`] enum and `docs/errors.md` from it. When `TM_ERROR_LOG` is set,
//! every [`Error`] constructed appends its code and a newline to that file
//! (the Verification Plan's error-code diff).

use std::fmt;
use std::io::Write as _;

use serde_json::{Value, json};

use crate::catalog::CATALOG;

include!(concat!(env!("OUT_DIR"), "/codes.rs"));

/// The gate at which a code can first be produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Gate {
    /// Gate A.
    A,
    /// Gate B.
    B,
    /// Gate C.
    C,
    /// Gate D.
    D,
}

impl Code {
    /// The code as written, for example `E1002`.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        CATALOG[self.index()].0
    }

    /// The catalog message.
    #[must_use]
    pub fn message(self) -> &'static str {
        CATALOG[self.index()].2
    }

    /// The earliest gate at which the code can be produced.
    #[must_use]
    pub fn gate(self) -> Gate {
        match CATALOG[self.index()].1 {
            'A' => Gate::A,
            'B' => Gate::B,
            'C' => Gate::C,
            _ => Gate::D,
        }
    }

    /// Look a code up by its text.
    #[must_use]
    pub fn parse(s: &str) -> Option<Code> {
        Code::ALL.iter().copied().find(|c| c.as_str() == s)
    }
}

impl fmt::Display for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One error, `{"code","message","rule","field","owner_command","subject"}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    /// The catalog code.
    pub code: Code,
    /// A human-readable message; may change between versions.
    pub message: String,
    /// The rule refused, for agent refusals.
    pub rule: Option<String>,
    /// A JSON Pointer (RFC 6901) into the document the error is about, or
    /// `None` when the input is not a JSON document.
    pub field: Option<String>,
    /// The owner command that would do what was refused, for agent refusals.
    pub owner_command: Option<String>,
    /// What the error is about (a handle, a path, a ref).
    pub subject: Option<String>,
}

impl Error {
    /// A new error with the given code and message. Appends the code to
    /// `$TM_ERROR_LOG` when that variable is set.
    pub fn new(code: Code, message: impl Into<String>) -> Error {
        if let Some(path) = std::env::var_os("TM_ERROR_LOG") {
            // Best effort: the log is a test aid and must never turn an
            // error into a different one.
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
            {
                let _ = writeln!(f, "{code}");
            }
        }
        Error {
            code,
            message: message.into(),
            rule: None,
            field: None,
            owner_command: None,
            subject: None,
        }
    }

    /// A new error about the member at `field` (a JSON Pointer).
    pub fn at(code: Code, field: impl Into<String>, message: impl Into<String>) -> Error {
        let mut e = Error::new(code, message);
        e.field = Some(field.into());
        e
    }

    /// Set the subject.
    #[must_use]
    pub fn with_subject(mut self, subject: impl Into<String>) -> Error {
        self.subject = Some(subject.into());
        self
    }

    /// Re-home a nested error inside an envelope line: the code becomes
    /// `E1002` and the pointer is prefixed (11.12).
    #[must_use]
    pub fn in_envelope(mut self, prefix: &str) -> Error {
        let inner = self.field.take().unwrap_or_default();
        let message = format!("{}: {}", self.code, self.message);
        let mut e = Error::at(Code::E1002, format!("{prefix}{inner}"), message);
        e.subject = self.subject;
        e
    }

    /// The 11.12 JSON object.
    #[must_use]
    pub fn to_json(&self) -> Value {
        json!({
            "code": self.code.as_str(),
            "message": self.message,
            "rule": self.rule,
            "field": self.field,
            "owner_command": self.owner_command,
            "subject": self.subject,
        })
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)?;
        if let Some(field) = &self.field {
            write!(f, " (at {field:?})")?;
        }
        Ok(())
    }
}

impl std::error::Error for Error {}

/// Escape one JSON Pointer reference token (RFC 6901: `~` to `~0`, `/` to
/// `~1`).
#[must_use]
pub fn pointer_token(s: &str) -> String {
    s.replace('~', "~0").replace('/', "~1")
}

/// Join a pointer and a member name or index.
#[must_use]
pub fn ptr(base: &str, token: impl fmt::Display) -> String {
    format!("{base}/{}", pointer_token(&token.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_round_trips() {
        for c in Code::ALL {
            assert_eq!(Code::parse(c.as_str()), Some(*c));
        }
        assert_eq!(Code::E1002.gate(), Gate::A);
        assert_eq!(Code::E8201.gate(), Gate::D);
    }

    #[test]
    fn pointers_escape() {
        assert_eq!(ptr("/body", "a/b~c"), "/body/a~1b~0c");
        assert_eq!(ptr("/ops", 1), "/ops/1");
    }
}
