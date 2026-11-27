//! The 11.19 settings table as a schema with validation (item 81), and the
//! resolved settings a command reads.
//!
//! Every setting is owner-set through `settings.set` and recorded by
//! `settings.changed`, whose `value` and `previous` are typed values
//! (11.6; a list-valued setting is a `list` of `str`). Unknown keys are
//! `E7001`; values failing validation are `E7002`. Gate A implements the
//! gate A rows; the others are accepted with their validation and read by
//! nothing until their gate.

use serde_json::{Value, json};
use tm_formats::typed_value::TypedValue;
use tm_fsfacts::manifest::{DEFAULT_IGNORE, Excludes};
use tm_index::{IndexTx, Param};

use crate::{Code, Error, Result};

/// A setting's type (11.19).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Type {
    /// `int`.
    Int,
    /// `bool`.
    Bool,
    /// `str`.
    Str,
    /// `list of str`.
    ListStr,
}

/// How a value is validated (11.19's Validation column).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Check {
    /// None.
    None,
    /// An integer from `lo` to `hi` inclusive.
    Range(i64, i64),
    /// An integer of at least `lo`.
    AtLeast(i64),
    /// An integer of at least `lo`, or unset (`keep.budget_mib`).
    AtLeastOrUnset(i64),
    /// An absolute path.
    AbsolutePath,
    /// Each entry parses as an exclude glob (item 28).
    Globs,
    /// A regular expression (gate C's `login_node.pattern`): accepted as a
    /// non-empty string in gate A, where nothing reads it and the
    /// workspace has no regex crate (D24); the reader validates it.
    Regex,
    /// Paths inside the repository (gate B's `files.agent_denied`): the
    /// 11.7 input-path form, checked by Files' rule in gate A.
    RepoPaths,
}

/// One 11.19 row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Def {
    /// The key.
    pub key: &'static str,
    /// The type.
    pub ty: Type,
    /// The default: an integer, `true`/`false`, a string, or a list, as
    /// 11.19 gives it; `None` for unset.
    pub default: Option<&'static str>,
    /// The validation.
    pub check: Check,
    /// The gate (`'A'` to `'D'`).
    pub gate: char,
}

/// The 11.19 table, in its order.
pub const DEFS: &[Def] = &[
    Def {
        key: "hashing.inline_threshold",
        ty: Type::Int,
        default: Some("64"),
        check: Check::Range(1, 65536),
        gate: 'A',
    },
    Def {
        key: "hashing.threads",
        ty: Type::Int,
        default: Some("1"),
        check: Check::Range(1, 256),
        gate: 'A',
    },
    Def {
        key: "hashing.bandwidth_mib",
        ty: Type::Int,
        default: Some("0"),
        check: Check::AtLeast(0),
        gate: 'A',
    },
    Def {
        key: "hashing.trust_ctime",
        ty: Type::Bool,
        default: Some("true"),
        check: Check::None,
        gate: 'A',
    },
    Def {
        key: "lock.heartbeat_seconds",
        ty: Type::Int,
        default: Some("5"),
        check: Check::Range(1, 60),
        gate: 'A',
    },
    Def {
        key: "lock.takeover_seconds",
        ty: Type::Int,
        default: Some("300"),
        check: Check::Range(30, 86400),
        gate: 'A',
    },
    Def {
        key: "keep.warn_mib",
        ty: Type::Int,
        default: Some("1024"),
        check: Check::AtLeast(1),
        gate: 'A',
    },
    Def {
        key: "keep.budget_mib",
        ty: Type::Int,
        default: None,
        check: Check::AtLeastOrUnset(1),
        gate: 'A',
    },
    Def {
        key: "checker.memory_limit_mib",
        ty: Type::Int,
        default: Some("2048"),
        check: Check::AtLeast(256),
        gate: 'A',
    },
    Def {
        key: "checker.temp_directory",
        ty: Type::Str,
        default: None,
        check: Check::AbsolutePath,
        gate: 'A',
    },
    Def {
        key: "checker.call_seconds",
        ty: Type::Int,
        default: Some("30"),
        check: Check::AtLeast(1),
        gate: 'A',
    },
    Def {
        key: "view.rows_per_page",
        ty: Type::Int,
        default: Some("500"),
        check: Check::AtLeast(1),
        gate: 'A',
    },
    Def {
        key: "manifest.ignore",
        ty: Type::ListStr,
        default: None,
        check: Check::Globs,
        gate: 'A',
    },
    Def {
        key: "login_node.pattern",
        ty: Type::Str,
        default: None,
        check: Check::Regex,
        gate: 'C',
    },
    Def {
        key: "approval.require_all",
        ty: Type::Bool,
        default: Some("false"),
        check: Check::None,
        gate: 'C',
    },
    Def {
        key: "run.late_output_wait",
        ty: Type::Int,
        default: Some("600"),
        check: Check::AtLeast(0),
        gate: 'C',
    },
    Def {
        key: "run.finish_login_limit_mib",
        ty: Type::Int,
        default: Some("1024"),
        check: Check::AtLeast(1),
        gate: 'C',
    },
    Def {
        key: "session.abandon_hours",
        ty: Type::Int,
        default: Some("72"),
        check: Check::AtLeast(1),
        gate: 'B',
    },
    Def {
        key: "session.rows_per_call",
        ty: Type::Int,
        default: Some("10000"),
        check: Check::AtLeast(1),
        gate: 'B',
    },
    Def {
        key: "session.bytes_per_call_mib",
        ty: Type::Int,
        default: Some("64"),
        check: Check::AtLeast(1),
        gate: 'B',
    },
    Def {
        key: "session.calls",
        ty: Type::Int,
        default: Some("5000"),
        check: Check::AtLeast(1),
        gate: 'B',
    },
    Def {
        key: "session.receipt_grace",
        ty: Type::Int,
        default: None,
        check: Check::AtLeast(0),
        gate: 'C',
    },
    Def {
        key: "harness.env_vars",
        ty: Type::ListStr,
        default: None,
        check: Check::None,
        gate: 'A',
    },
    Def {
        key: "files.agent_denied",
        ty: Type::ListStr,
        default: None,
        check: Check::RepoPaths,
        gate: 'B',
    },
];

/// The default `harness.env_vars` (OQ7).
pub const DEFAULT_HARNESS_ENV_VARS: &[&str] = &["AGENTBOX_SESSION", "CLAUDECODE"];

/// The row for a key.
///
/// # Errors
/// `E7001` for an unknown key.
pub fn def(key: &str) -> Result<&'static Def> {
    DEFS.iter()
        .find(|d| d.key == key)
        .ok_or_else(|| Error::code(Code::E7001, format!("unknown setting {key:?}")))
}

fn e7002(key: &str, why: impl std::fmt::Display) -> Error {
    Error::code(Code::E7002, format!("{key}: {why}"))
}

/// Parse the command line's `VALUE...` for `key` into its typed value
/// (list-valued keys take repeated values and replace the whole list).
///
/// # Errors
/// `E7001` for an unknown key; `E7002` for a value that does not parse or
/// fails the row's validation.
pub fn parse_value(key: &str, values: &[String]) -> Result<TypedValue> {
    let d = def(key)?;
    let one = || -> Result<&String> {
        match values {
            [v] => Ok(v),
            _ => Err(e7002(
                key,
                format!("takes one value, {} given", values.len()),
            )),
        }
    };
    let tv = match d.ty {
        Type::Int => {
            let text = one()?;
            let n: i64 = text
                .parse()
                .map_err(|_| e7002(key, format!("{text:?} is not an integer")))?;
            TypedValue::Int(n.into())
        }
        Type::Bool => match one()?.as_str() {
            "true" => TypedValue::Bool(true),
            "false" => TypedValue::Bool(false),
            other => return Err(e7002(key, format!("{other:?} is not `true` or `false`"))),
        },
        Type::Str => TypedValue::str_nfc(one()?),
        Type::ListStr => {
            if values.is_empty() {
                return Err(e7002(key, "a list takes at least one value"));
            }
            TypedValue::List(values.iter().map(|v| TypedValue::str_nfc(v)).collect())
        }
    };
    validate(d, &tv)?;
    Ok(tv)
}

/// Check a typed value against its row (what `settings.set` enforces).
///
/// # Errors
/// `E7002`.
pub fn validate(d: &Def, v: &TypedValue) -> Result<()> {
    let key = d.key;
    let int = |v: &TypedValue| -> Result<i64> {
        match v {
            TypedValue::Int(n) => {
                i64::try_from(n.clone()).map_err(|_| e7002(key, "out of the integer range"))
            }
            _ => Err(e7002(key, "an integer is required")),
        }
    };
    match d.check {
        Check::None => match (d.ty, v) {
            (Type::Bool, TypedValue::Bool(_))
            | (Type::Str, TypedValue::Str(_))
            | (Type::Int, TypedValue::Int(_)) => Ok(()),
            (Type::ListStr, TypedValue::List(items)) => items
                .iter()
                .all(|i| matches!(i, TypedValue::Str(_)))
                .then_some(())
                .ok_or_else(|| e7002(key, "a list of strings is required")),
            _ => Err(e7002(key, "wrong type")),
        },
        Check::Range(lo, hi) => {
            let n = int(v)?;
            (lo..=hi)
                .contains(&n)
                .then_some(())
                .ok_or_else(|| e7002(key, format!("{n} is not between {lo} and {hi}")))
        }
        Check::AtLeast(lo) | Check::AtLeastOrUnset(lo) => {
            let n = int(v)?;
            (n >= lo)
                .then_some(())
                .ok_or_else(|| e7002(key, format!("{n} is below {lo}")))
        }
        Check::AbsolutePath => match v {
            TypedValue::Str(s) if s.starts_with('/') => Ok(()),
            TypedValue::Str(s) => Err(e7002(key, format!("{s:?} is not an absolute path"))),
            _ => Err(e7002(key, "a path is required")),
        },
        Check::Regex => match v {
            TypedValue::Str(s) if !s.is_empty() => Ok(()),
            _ => Err(e7002(key, "a non-empty pattern is required")),
        },
        Check::Globs => {
            let items = strings(v).ok_or_else(|| e7002(key, "a list of globs is required"))?;
            Excludes::parse(&items).map_err(|e| e7002(key, e))?;
            Ok(())
        }
        Check::RepoPaths => {
            let items = strings(v).ok_or_else(|| e7002(key, "a list of paths is required"))?;
            for p in &items {
                tm_files::identity::check_input_path(p).map_err(|e| e7002(key, e))?;
            }
            Ok(())
        }
    }
}

fn strings(v: &TypedValue) -> Option<Vec<String>> {
    match v {
        TypedValue::List(items) => items
            .iter()
            .map(|i| match i {
                TypedValue::Str(s) => Some(s.clone()),
                _ => None,
            })
            .collect(),
        _ => None,
    }
}

/// The resolved settings: every key's current typed value, from the index
/// (`settings` table) with 11.19's defaults for the rest.
#[derive(Debug, Clone, Default)]
pub struct Settings {
    values: Vec<(String, Option<TypedValue>, Option<String>)>,
}

impl Settings {
    /// Read the `settings` table.
    ///
    /// # Errors
    /// `Error::Index`.
    pub fn load(tx: &mut IndexTx<'_>) -> Result<Settings> {
        let rows = tx.query_rows("SELECT key, value, event_id FROM settings", &[])?;
        let mut values = Vec::new();
        for r in rows {
            let key = r.text(0).unwrap_or_default().to_owned();
            let value = match r.text(1) {
                Some(text) => {
                    let v: Value = serde_json::from_str(text).unwrap_or(Value::Null);
                    TypedValue::from_json(&v, "").ok()
                }
                None => None,
            };
            values.push((key, value, r.text(2).map(str::to_owned)));
        }
        Ok(Settings { values })
    }

    /// Settings with every default (no record read).
    #[must_use]
    pub fn defaults() -> Settings {
        Settings::default()
    }

    /// The current value of `key`: the set value, else the 11.19 default,
    /// else `None` (unset).
    #[must_use]
    pub fn get(&self, key: &str) -> Option<TypedValue> {
        if let Some((_, v, _)) = self.values.iter().find(|(k, _, _)| k == key) {
            return v.clone();
        }
        let d = DEFS.iter().find(|d| d.key == key)?;
        default_value(d)
    }

    /// The event that set `key`, if one did.
    #[must_use]
    pub fn set_by(&self, key: &str) -> Option<&str> {
        self.values
            .iter()
            .find(|(k, _, _)| k == key)
            .and_then(|(_, _, e)| e.as_deref())
    }

    /// An integer setting (its default when unset).
    #[must_use]
    pub fn int(&self, key: &str) -> Option<i64> {
        match self.get(key)? {
            TypedValue::Int(n) => i64::try_from(n).ok(),
            _ => None,
        }
    }

    /// An integer setting as `u64`, `fallback` when unset.
    #[must_use]
    pub fn u64_or(&self, key: &str, fallback: u64) -> u64 {
        self.int(key)
            .and_then(|n| u64::try_from(n).ok())
            .unwrap_or(fallback)
    }

    /// A boolean setting, `fallback` when unset.
    #[must_use]
    pub fn bool_or(&self, key: &str, fallback: bool) -> bool {
        match self.get(key) {
            Some(TypedValue::Bool(b)) => b,
            _ => fallback,
        }
    }

    /// A string setting.
    #[must_use]
    pub fn str(&self, key: &str) -> Option<String> {
        match self.get(key)? {
            TypedValue::Str(s) => Some(s),
            _ => None,
        }
    }

    /// A list setting's strings.
    #[must_use]
    pub fn list(&self, key: &str) -> Option<Vec<String>> {
        self.get(key).as_ref().and_then(strings)
    }

    /// `manifest.ignore`, the default list when unset (5.5).
    #[must_use]
    pub fn manifest_ignore(&self) -> Vec<String> {
        self.list("manifest.ignore")
            .unwrap_or_else(|| DEFAULT_IGNORE.iter().map(|s| (*s).to_owned()).collect())
    }

    /// `harness.env_vars`, OQ7's default when unset.
    #[must_use]
    pub fn harness_env_vars(&self) -> Vec<String> {
        self.list("harness.env_vars").unwrap_or_else(|| {
            DEFAULT_HARNESS_ENV_VARS
                .iter()
                .map(|s| (*s).to_owned())
                .collect()
        })
    }

    /// The hashing settings (item 27).
    #[must_use]
    pub fn hash_settings(&self) -> tm_fsfacts::hash::HashSettings {
        tm_fsfacts::hash::HashSettings {
            threads: usize::try_from(self.u64_or("hashing.threads", 1)).unwrap_or(1),
            bandwidth_mib: self.u64_or("hashing.bandwidth_mib", 0),
        }
    }

    /// The re-hash settings (item 30).
    #[must_use]
    pub fn rehash_settings(&self) -> tm_fsfacts::needs_rehash::RehashSettings {
        tm_fsfacts::needs_rehash::RehashSettings {
            inline_threshold_mib: self.u64_or("hashing.inline_threshold", 64),
            trust_ctime: self.bool_or("hashing.trust_ctime", true),
        }
    }

    /// The lock options (3.1; OQ2).
    #[must_use]
    pub fn lock_options(&self) -> tm_record::LockOptions {
        tm_record::LockOptions {
            heartbeat: std::time::Duration::from_secs(self.u64_or("lock.heartbeat_seconds", 5)),
            takeover: std::time::Duration::from_secs(self.u64_or("lock.takeover_seconds", 300)),
            ..tm_record::LockOptions::default()
        }
    }

    /// The engine settings (11.19; `checker.temp_directory` defaults to
    /// the parse cache directory the caller names, item 76).
    #[must_use]
    pub fn engine_settings(&self, parse_cache_dir: &std::path::Path) -> tm_duckdb::EngineSettings {
        tm_duckdb::EngineSettings {
            memory_limit_mib: self.u64_or("checker.memory_limit_mib", 2048),
            temp_directory: self
                .str("checker.temp_directory")
                .unwrap_or_else(|| parse_cache_dir.to_string_lossy().into_owned()),
            call_seconds: self.u64_or("checker.call_seconds", 30),
        }
    }

    /// `view.rows_per_page`.
    #[must_use]
    pub fn rows_per_page(&self) -> usize {
        usize::try_from(self.u64_or("view.rows_per_page", 500)).unwrap_or(500)
    }

    /// `settings.show`: every key with its value, default, and the event
    /// that set it (section 10).
    #[must_use]
    pub fn show(&self) -> Value {
        Value::Array(
            DEFS.iter()
                .map(|d| {
                    json!({
                        "key": d.key,
                        "value": self.get(d.key).map_or(Value::Null, |v| v.to_json()),
                        "default": default_value(d).map_or(Value::Null, |v| v.to_json()),
                        "set_by": self.set_by(d.key),
                        "gate": d.gate.to_string(),
                    })
                })
                .collect(),
        )
    }
}

/// The 11.19 default of a row as a typed value, `None` for unset.
#[must_use]
pub fn default_value(d: &Def) -> Option<TypedValue> {
    match d.key {
        "manifest.ignore" => Some(TypedValue::List(
            DEFAULT_IGNORE
                .iter()
                .map(|s| TypedValue::str_nfc(s))
                .collect(),
        )),
        "harness.env_vars" => Some(TypedValue::List(
            DEFAULT_HARNESS_ENV_VARS
                .iter()
                .map(|s| TypedValue::str_nfc(s))
                .collect(),
        )),
        _ => {
            let text = d.default?;
            Some(match d.ty {
                Type::Int => TypedValue::Int(text.parse::<i64>().unwrap_or(0).into()),
                Type::Bool => TypedValue::Bool(text == "true"),
                Type::Str | Type::ListStr => TypedValue::str_nfc(text),
            })
        }
    }
}

/// The `settings` row for the fold: canonical JSON of the value, `NULL`
/// for an unset.
pub(crate) fn fold_settings_changed(
    tx: &mut IndexTx<'_>,
    event_id: &str,
    body: &serde_json::Map<String, Value>,
) -> Result<()> {
    let key = body.get("key").and_then(Value::as_str).unwrap_or_default();
    let value = match body.get("value") {
        None | Some(Value::Null) => None,
        Some(v) => Some(tm_formats::canonical::canonical(v)?),
    };
    tx.execute(
        "INSERT OR REPLACE INTO settings (key, value, event_id) VALUES (?, ?, ?)",
        &[Param::from(key), Param::from(value), Param::from(event_id)],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_validates() {
        let v = parse_value("hashing.threads", &["4".into()]).unwrap();
        assert_eq!(v, TypedValue::Int(4.into()));
        assert_eq!(
            parse_value("hashing.threads", &["0".into()])
                .unwrap_err()
                .catalog_code(),
            Some(Code::E7002)
        );
        assert_eq!(
            parse_value("nope", &["1".into()])
                .unwrap_err()
                .catalog_code(),
            Some(Code::E7001)
        );
        assert_eq!(
            parse_value("hashing.trust_ctime", &["no".into()])
                .unwrap_err()
                .catalog_code(),
            Some(Code::E7002)
        );
        let globs =
            parse_value("manifest.ignore", &["*.swp".into(), ".snakemake/".into()]).unwrap();
        assert!(matches!(globs, TypedValue::List(ref l) if l.len() == 2));
        assert_eq!(
            parse_value("manifest.ignore", &["a/b/".into()])
                .unwrap_err()
                .catalog_code(),
            Some(Code::E7002)
        );
        assert_eq!(
            parse_value("checker.temp_directory", &["rel/path".into()])
                .unwrap_err()
                .catalog_code(),
            Some(Code::E7002)
        );
        assert_eq!(
            parse_value("files.agent_denied", &["../x".into()])
                .unwrap_err()
                .catalog_code(),
            Some(Code::E7002)
        );
        let s = Settings::defaults();
        assert_eq!(s.int("view.rows_per_page"), Some(500));
        assert_eq!(s.int("keep.budget_mib"), None);
        assert_eq!(s.harness_env_vars(), vec!["AGENTBOX_SESSION", "CLAUDECODE"]);
        assert_eq!(s.manifest_ignore().len(), DEFAULT_IGNORE.len());
        assert_eq!(s.show().as_array().unwrap().len(), DEFS.len());
    }
}
