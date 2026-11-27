//! Who is acting (item 72; D23; 11.2 `actor` and `src`).
//!
//! Gate A has one actor, the owner, who proves it with an [`OwnerCap`]
//! (rule 4). The `src` facts of an owner action are detected here: whether
//! standard input is a terminal and whether a known agent-harness variable
//! is set (the `harness.env_vars` setting, default `AGENTBOX_SESSION` and
//! `CLAUDECODE`, OQ7). `AGENTBOX_AGENT` names the harness when present;
//! 11.2's `src` has no slot for it, so the name travels with the detected
//! source for the command's output (the `possibly agent-issued` label) and
//! is not stored.

use std::io::IsTerminal as _;

use tm_formats::envelope::{self, ActorKind, SourceKind};
use tm_owner_cap::OwnerCap;

/// The actor of a Core request (4.4). Gate A: the owner only; the session
/// and viewer variants arrive with gates B and D.
pub enum Actor<'a> {
    /// The owner, holding the capability.
    Owner(&'a OwnerCap),
}

impl Actor<'_> {
    /// The 11.2 `actor` object, `id` the record's `owner_id`.
    #[must_use]
    pub fn envelope_actor(&self, owner_id: &str) -> envelope::Actor {
        match self {
            Actor::Owner(_) => envelope::Actor {
                kind: ActorKind::Owner,
                id: owner_id.to_owned(),
            },
        }
    }

    /// The capability, for the Record append.
    #[must_use]
    pub fn cap(&self) -> &OwnerCap {
        match self {
            Actor::Owner(c) => c,
        }
    }
}

/// The D23 facts of an owner action, detected once per command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    /// `cli` or `static-view` (`tm apply`).
    pub source: SourceKind,
    /// Standard input is a terminal.
    pub tty: bool,
    /// A known harness variable is set.
    pub harness_env: bool,
    /// `AGENTBOX_AGENT`'s value when set: the harness's name, for the
    /// command's output (not stored, 11.2).
    pub harness_name: Option<String>,
}

impl Source {
    /// Detect the facts from the process: `isatty` on standard input and
    /// the `harness.env_vars` list.
    #[must_use]
    pub fn detect(source: SourceKind, harness_env_vars: &[String]) -> Source {
        let harness_env = harness_env_vars
            .iter()
            .any(|v| std::env::var_os(v).is_some_and(|x| !x.is_empty()));
        Source {
            source,
            tty: std::io::stdin().is_terminal(),
            harness_env,
            harness_name: std::env::var("AGENTBOX_AGENT")
                .ok()
                .filter(|s| !s.is_empty()),
        }
    }

    /// A source for a command with no terminal and no harness (tests).
    #[must_use]
    pub fn plain(source: SourceKind) -> Source {
        Source {
            source,
            tty: false,
            harness_env: false,
            harness_name: None,
        }
    }

    /// The 11.2 `src` object.
    #[must_use]
    pub fn envelope_source(&self) -> envelope::Source {
        envelope::Source {
            source: self.source,
            tty: self.tty,
            harness_env: self.harness_env,
            confirmed_by: None,
        }
    }

    /// 5.6's `possibly agent-issued` label applies: a harness variable
    /// was set.
    #[must_use]
    pub fn possibly_agent_issued(&self) -> bool {
        self.harness_env
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_the_harness_variable_list() {
        // `PATH` is set in every test environment; the second name is not.
        let s = Source::detect(SourceKind::Cli, &["PATH".into()]);
        assert!(s.harness_env);
        assert!(s.possibly_agent_issued());
        let s = Source::detect(SourceKind::Cli, &["TM_TEST_HARNESS_VAR_UNSET".into()]);
        assert!(!s.harness_env);
        let cap = OwnerCap::for_tests();
        let a = Actor::Owner(&cap).envelope_actor("o-1");
        assert_eq!(a.kind, ActorKind::Owner);
        assert_eq!(a.id, "o-1");
    }
}
