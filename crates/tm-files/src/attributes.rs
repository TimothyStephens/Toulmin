//! The 6.1 to 6.3 state machine (item 46): which owner actions a file in a
//! given state admits, and `E3201` for the rest.

use crate::identity::Level;
use crate::{Code, Error, Result};

/// A version's kept state (6.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kept {
    /// Not kept; bytes available only while on disk.
    No,
    /// Bytes copied into the record.
    Bytes,
    /// The owner chose to keep only the fingerprint (`evidence not kept`).
    FingerprintOnly,
}

impl Kept {
    /// True for either kept form.
    #[must_use]
    pub fn is_kept(self) -> bool {
        !matches!(self, Kept::No)
    }
}

/// The part of a file's state the transitions look at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct State {
    /// Tracked or registered.
    pub level: Level,
    /// An input identity (outputs arrive with gate C).
    pub is_input: bool,
    /// Retired.
    pub retired: bool,
    /// Marked removable.
    pub removable: bool,
    /// The latest version's kept state, or `None` when there is no version.
    pub latest: Option<Kept>,
}

/// An owner action on a file (5.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// `tm file register` on a tracked or new identity.
    Register,
    /// `tm file hash`, with `--again` or not (section 9's `file.hash` row).
    Hash {
        /// Hash again although a version exists.
        again: bool,
    },
    /// `tm file keep`, with `--fingerprint-only` or not.
    Keep {
        /// Keep only the fingerprint.
        fingerprint_only: bool,
    },
    /// `tm file removable`.
    MarkRemovable,
    /// `tm file restore`.
    Restore,
    /// `tm file rename`.
    Rename,
    /// `tm file panels`, given whether the version is an image (item 49).
    SetPanels {
        /// The version's bytes start with an image signature.
        is_image: bool,
    },
    /// Automatic retirement (gate C).
    Retire,
}

impl Action {
    fn name(self) -> &'static str {
        match self {
            Action::Register => "register",
            Action::Hash { .. } => "hash",
            Action::Keep { .. } => "keep",
            Action::MarkRemovable => "removable",
            Action::Restore => "restore",
            Action::Rename => "rename",
            Action::SetPanels { .. } => "panels",
            Action::Retire => "retire",
        }
    }
}

impl State {
    /// Apply `action` (item 46's table), returning the state after it.
    ///
    /// # Errors
    /// `E3201` naming the state and the action when 6.3 does not allow it;
    /// `E3005` for `hash` without `--again` on an identity that has a
    /// version; `E3301` for `panels` on a version that is not an image.
    pub fn apply(self, action: Action) -> Result<State> {
        let refuse = |why: &str| {
            Err(Error::code(
                Code::E3201,
                format!("cannot {} this file: {why}", action.name()),
            ))
        };
        match action {
            Action::Register => {
                if self.retired {
                    return refuse("a retired file cannot be registered again; restore it");
                }
                if self.level == Level::Registered {
                    return refuse("it is already registered");
                }
                Ok(State {
                    level: Level::Registered,
                    ..self
                })
            }
            Action::Hash { again } => {
                if self.level != Level::Registered {
                    return refuse("hashing follows registration (`tm file register`)");
                }
                if self.latest.is_some() && !again {
                    return Err(Error::code(
                        Code::E3005,
                        "already hashed: this file has a version; use `--again` to hash it again",
                    ));
                }
                Ok(self)
            }
            Action::Keep { fingerprint_only } => {
                if self.level != Level::Registered {
                    return refuse("keeping requires registration first (6.3)");
                }
                match self.latest {
                    None => refuse("it has no version yet (`tm file hash`)"),
                    Some(k) if k.is_kept() => refuse("its latest version is already kept"),
                    Some(_) => Ok(State {
                        latest: Some(if fingerprint_only {
                            Kept::FingerprintOnly
                        } else {
                            Kept::Bytes
                        }),
                        ..self
                    }),
                }
            }
            Action::MarkRemovable => {
                if self.removable {
                    return refuse("it is already marked removable");
                }
                Ok(State {
                    removable: true,
                    ..self
                })
            }
            Action::Restore => {
                if !self.retired {
                    return refuse("it is not retired");
                }
                Ok(State {
                    retired: false,
                    ..self
                })
            }
            Action::Rename => {
                if !self.is_input {
                    return refuse(
                        "only an input identity is renamed; an output's path is the declaration's",
                    );
                }
                if self.retired {
                    return refuse("it is retired; restore it first");
                }
                Ok(self)
            }
            Action::SetPanels { is_image } => {
                if self.latest.is_none() {
                    return refuse("it has no version yet");
                }
                if !is_image {
                    return Err(Error::code(
                        Code::E3301,
                        "panels are set on an image version (PNG, JPEG, GIF, TIFF, BMP, or WebP)",
                    ));
                }
                Ok(self)
            }
            Action::Retire => {
                if self.retired {
                    return refuse("it is already retired");
                }
                Ok(State {
                    retired: true,
                    ..self
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registered(latest: Option<Kept>) -> State {
        State {
            level: Level::Registered,
            is_input: true,
            retired: false,
            removable: false,
            latest,
        }
    }

    #[test]
    fn allowed_and_refused_transitions() {
        let tracked = State {
            level: Level::Tracked,
            ..registered(None)
        };
        assert_eq!(
            tracked.apply(Action::Register).unwrap().level,
            Level::Registered
        );
        let code = |r: Result<State>| r.unwrap_err().catalog_code();
        assert_eq!(
            code(registered(None).apply(Action::Register)),
            Some(Code::E3201)
        );
        let retired = State {
            retired: true,
            ..tracked
        };
        assert_eq!(code(retired.apply(Action::Register)), Some(Code::E3201));
        assert!(!retired.apply(Action::Restore).unwrap().retired);
        assert_eq!(
            code(registered(None).apply(Action::Restore)),
            Some(Code::E3201)
        );
        // Keep: needs registration and a version, once.
        assert_eq!(
            code(tracked.apply(Action::Keep {
                fingerprint_only: false
            })),
            Some(Code::E3201)
        );
        assert_eq!(
            code(registered(None).apply(Action::Keep {
                fingerprint_only: false
            })),
            Some(Code::E3201)
        );
        let kept = registered(Some(Kept::No))
            .apply(Action::Keep {
                fingerprint_only: true,
            })
            .unwrap();
        assert_eq!(kept.latest, Some(Kept::FingerprintOnly));
        assert_eq!(
            code(kept.apply(Action::Keep {
                fingerprint_only: false
            })),
            Some(Code::E3201)
        );
        // Removable once.
        let removable = registered(None).apply(Action::MarkRemovable).unwrap();
        assert!(removable.removable);
        assert_eq!(
            code(removable.apply(Action::MarkRemovable)),
            Some(Code::E3201)
        );
        // Rename: inputs that are not retired.
        assert!(registered(None).apply(Action::Rename).is_ok());
        let output = State {
            is_input: false,
            ..registered(None)
        };
        assert_eq!(code(output.apply(Action::Rename)), Some(Code::E3201));
        assert_eq!(
            code(
                State {
                    retired: true,
                    ..registered(None)
                }
                .apply(Action::Rename)
            ),
            Some(Code::E3201)
        );
        // Panels: an image version.
        assert!(
            registered(Some(Kept::No))
                .apply(Action::SetPanels { is_image: true })
                .is_ok()
        );
        assert_eq!(
            code(registered(Some(Kept::No)).apply(Action::SetPanels { is_image: false })),
            Some(Code::E3301)
        );
        assert_eq!(
            code(registered(None).apply(Action::SetPanels { is_image: true })),
            Some(Code::E3201)
        );
        // Retire once.
        let r = registered(None).apply(Action::Retire).unwrap();
        assert!(r.retired);
        assert_eq!(code(r.apply(Action::Retire)), Some(Code::E3201));
    }

    #[test]
    fn hash_transition() {
        let code = |r: Result<State>| r.unwrap_err().catalog_code();
        let tracked = State {
            level: Level::Tracked,
            ..registered(None)
        };
        // Hash: registered with no version, or again; E3005 otherwise.
        assert!(
            registered(None)
                .apply(Action::Hash { again: false })
                .is_ok()
        );
        assert_eq!(
            code(registered(Some(Kept::No)).apply(Action::Hash { again: false })),
            Some(Code::E3005)
        );
        assert!(
            registered(Some(Kept::No))
                .apply(Action::Hash { again: true })
                .is_ok()
        );
        assert_eq!(
            code(tracked.apply(Action::Hash { again: true })),
            Some(Code::E3201)
        );
    }
}
