//! Citation objects (design 11.23, item 19): the enum, its JSON encoding,
//! and the parser for the CLI spelling with its escapes.
//!
//! JSON validation codes: `E6002` absent, extra, or unknown member (an
//! unknown `kind` included), `E6004` wrong JSON type, `E1004` malformed
//! typed value. Every CLI-spelling failure is `E6007` with no `field`.

use serde_json::{Map, Value, json};

use crate::errors::{Code, Error, ptr};
use crate::id::{VersionHandle, positive};
use crate::parse_spec::ParseSpec;
use crate::schema::{self, Codes};
use crate::typed_value::{Tag, TypedValue};

const CODES: Codes = Codes {
    missing: Code::E6002,
    wrong_type: Code::E6004,
};

/// The reserved kinds (11.13), stored as given and not interpreted.
pub const RESERVED_KINDS: &[&str] = &["quote", "page_region", "image_region"];

/// One citation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Citation {
    /// Cells of a table: key tuples and optionally columns.
    Cells {
        /// The cited version.
        version: VersionHandle,
        /// Key tuples, each in the spec's key-column order.
        keys: Vec<Vec<TypedValue>>,
        /// Cited columns, or `None` for whole rows.
        columns: Option<Vec<String>>,
    },
    /// A whole file.
    WholeFile {
        /// The cited version.
        version: VersionHandle,
    },
    /// A 1-based PDF page.
    Page {
        /// The cited version.
        version: VersionHandle,
        /// The page.
        page: u64,
    },
    /// A registered image panel.
    Panel {
        /// The cited version.
        version: VersionHandle,
        /// The panel name.
        panel: String,
    },
    /// A reserved kind, kept as given.
    Reserved(Value),
}

fn version(m: &Map<String, Value>, at: &str) -> Result<VersionHandle, Error> {
    let s = schema::string(m, "version", at, CODES)?;
    VersionHandle::parse(s).ok_or_else(|| {
        Error::at(
            Code::E6004,
            ptr(at, "version"),
            format!("{s:?} is not a version handle"),
        )
    })
}

fn key_value_ok(v: &TypedValue) -> bool {
    !matches!(v.tag(), Tag::Missing | Tag::Nan | Tag::Inf | Tag::List)
}

impl Citation {
    /// Validate one 11.23 object; `at` is its pointer.
    ///
    /// # Errors
    /// `E6002`, `E6004`, or `E1004` with `field` under `at`.
    pub fn from_json(v: &Value, at: &str) -> Result<Citation, Error> {
        let m = schema::object(v, at, CODES, "a citation")?;
        if !m.contains_key("kind") {
            return Err(Error::at(
                Code::E6002,
                ptr(at, "kind"),
                "a citation names its `kind`",
            ));
        }
        let kind = schema::string(m, "kind", at, CODES)?;
        if RESERVED_KINDS.contains(&kind) {
            return Ok(Citation::Reserved(v.clone()));
        }
        Ok(match kind {
            "cells" => {
                schema::keys(m, at, &["kind", "version", "keys", "columns"], &[], CODES)?;
                let version = version(m, at)?;
                let ka = schema::array(m, "keys", at, false, CODES)?;
                let kat = ptr(at, "keys");
                if ka.is_empty() {
                    return Err(Error::at(
                        Code::E6002,
                        kat,
                        "`keys` names at least one key tuple",
                    ));
                }
                let mut keys = Vec::with_capacity(ka.len());
                for (i, t) in ka.iter().enumerate() {
                    let tat = ptr(&kat, i);
                    let Some(items) = t.as_array() else {
                        return Err(Error::at(Code::E6004, tat, "a key tuple is a list"));
                    };
                    if items.is_empty() {
                        return Err(Error::at(Code::E6002, tat, "a key tuple is not empty"));
                    }
                    let mut tuple = Vec::with_capacity(items.len());
                    for (j, item) in items.iter().enumerate() {
                        let vat = ptr(&tat, j);
                        let tv = TypedValue::from_json(item, &vat)?;
                        if !key_value_ok(&tv) {
                            return Err(Error::at(
                                Code::E6004,
                                vat,
                                format!("a key value cannot be {:?}", tv.tag()),
                            ));
                        }
                        tuple.push(tv);
                    }
                    keys.push(tuple);
                }
                let columns = if m["columns"].is_null() {
                    None
                } else {
                    Some(schema::string_list(m, "columns", at, true, true, CODES)?)
                };
                Citation::Cells {
                    version,
                    keys,
                    columns,
                }
            }
            "whole_file" => {
                schema::keys(m, at, &["kind", "version"], &[], CODES)?;
                Citation::WholeFile {
                    version: version(m, at)?,
                }
            }
            "page" => {
                schema::keys(m, at, &["kind", "version", "page"], &[], CODES)?;
                let version = version(m, at)?;
                Citation::Page {
                    version,
                    page: schema::integer(m, "page", at, 1, CODES)?,
                }
            }
            "panel" => {
                schema::keys(m, at, &["kind", "version", "panel"], &[], CODES)?;
                let version = version(m, at)?;
                Citation::Panel {
                    version,
                    panel: schema::nonempty_string(m, "panel", at, CODES)?.to_owned(),
                }
            }
            other => {
                return Err(Error::at(
                    Code::E6002,
                    ptr(at, "kind"),
                    format!("unknown citation kind {other:?}"),
                ));
            }
        })
    }

    /// The JSON encoding.
    #[must_use]
    pub fn to_json(&self) -> Value {
        match self {
            Citation::Cells {
                version,
                keys,
                columns,
            } => json!({
                "kind": "cells",
                "version": version.to_string(),
                "keys": keys.iter().map(|t| t.iter().map(TypedValue::to_json).collect::<Vec<_>>()).collect::<Vec<_>>(),
                "columns": columns,
            }),
            Citation::WholeFile { version } => {
                json!({"kind": "whole_file", "version": version.to_string()})
            }
            Citation::Page { version, page } => {
                json!({"kind": "page", "version": version.to_string(), "page": page})
            }
            Citation::Panel { version, panel } => {
                json!({"kind": "panel", "version": version.to_string(), "panel": panel})
            }
            Citation::Reserved(v) => v.clone(),
        }
    }

    /// Parse the CLI spelling (11.23). `spec` is the cited version's parse
    /// spec, if it has one. Several `cells` arguments for the same version
    /// are separate citations here; joining them is the caller's.
    ///
    /// # Errors
    /// `E6007` with no `field` for anything 11.23 does not allow.
    #[allow(
        clippy::single_match_else,
        reason = "the two spellings are parallel cases"
    )]
    pub fn parse_cli(text: &str, spec: Option<&ParseSpec>) -> Result<Citation, Error> {
        let bad = |msg: String| Error::new(Code::E6007, msg).with_subject(text);
        let (handle, rest) = match text.find('#') {
            Some(i) => (&text[..i], Some(&text[i..])),
            None => (text, None),
        };
        let version = VersionHandle::parse(handle)
            .ok_or_else(|| bad(format!("{handle:?} is not a version handle")))?;
        let Some(rest) = rest else {
            return Ok(Citation::WholeFile { version });
        };
        let segments = split_segments(rest).map_err(bad)?;
        match spec {
            Some(spec) => {
                if segments.len() != spec.key.len() {
                    return Err(bad(format!(
                        "expected one #<column>=<value> for each of the {} key columns",
                        spec.key.len()
                    )));
                }
                let mut tuple = Vec::with_capacity(segments.len());
                for ((col, val), key) in segments.iter().zip(&spec.key) {
                    if col != key {
                        return Err(bad(format!("expected key column {key:?}, found {col:?}")));
                    }
                    let ty = spec
                        .column(key)
                        .map(|c| c.ty)
                        .ok_or_else(|| bad(format!("no column {key:?}")))?;
                    let tv = TypedValue::from_text(val, ty)
                        .ok_or_else(|| bad(format!("{val:?} is not a {} value", ty.as_str())))?;
                    tuple.push(tv);
                }
                Ok(Citation::Cells {
                    version,
                    keys: vec![tuple],
                    columns: None,
                })
            }
            None => {
                let [(name, val)] = segments.as_slice() else {
                    return Err(bad(
                        "without a parse spec only #page=<n> or #panel=<name> follows the handle"
                            .into(),
                    ));
                };
                match name.as_str() {
                    "page" => Ok(Citation::Page {
                        version,
                        page: positive(val)
                            .ok_or_else(|| bad(format!("{val:?} is not a page number")))?,
                    }),
                    "panel" if !val.is_empty() => Ok(Citation::Panel {
                        version,
                        panel: crate::sort_order::nfc(val),
                    }),
                    _ => Err(bad(format!("{name:?} is not page or panel"))),
                }
            }
        }
    }
}

/// Split `#a=1#b=2` into `(column, value)` pairs, unescaping values.
fn split_segments(rest: &str) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    let mut chars = rest.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '#' {
            return Err("expected '#'".into());
        }
        let mut name = String::new();
        loop {
            match chars.next() {
                Some('=') => break,
                Some('#' | '\\') | None => {
                    return Err(format!("expected '=' after column name {name:?}"));
                }
                Some(c) => name.push(c),
            }
        }
        if name.is_empty() {
            return Err("empty column name".into());
        }
        let mut value = String::new();
        while let Some(&c) = chars.peek() {
            match c {
                '#' => break,
                '=' => return Err("an '=' inside a value is written \\=".into()),
                '\\' => {
                    chars.next();
                    match chars.next() {
                        Some(e @ ('#' | '=' | '\\')) => value.push(e),
                        Some(e) => return Err(format!("unknown escape \\{e}")),
                        None => return Err("a trailing backslash".into()),
                    }
                }
                c => {
                    chars.next();
                    value.push(c);
                }
            }
        }
        out.push((name, value));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec2() -> ParseSpec {
        ParseSpec::from_json(&json!({
            "spec":1,"format":"csv","delimiter":",","header":true,
            "columns":[{"name":"sample","type":"str","missing":[]},{"name":"page","type":"int","missing":[]}],
            "key":["sample","page"],"nan":"missing","inf":"missing","encoding":"utf-8"
        }))
        .unwrap()
    }

    #[test]
    fn cli_forms() {
        assert_eq!(
            Citation::parse_cli("F7@v2", None).unwrap(),
            Citation::WholeFile {
                version: VersionHandle::parse("F7@v2").unwrap()
            }
        );
        assert!(matches!(
            Citation::parse_cli("F7@v2#page=3", None).unwrap(),
            Citation::Page { page: 3, .. }
        ));
        assert!(matches!(
            Citation::parse_cli("F7@v2#panel=B", None).unwrap(),
            Citation::Panel { .. }
        ));
        // With a spec, page is an ordinary key column.
        let c = Citation::parse_cli(r"F7@v2#sample=a\#b\=c\\d#page=3", Some(&spec2())).unwrap();
        assert_eq!(
            c.to_json(),
            json!({"kind":"cells","version":"F7@v2","keys":[[{"t":"str","v":"a#b=c\\d"},{"t":"int","v":"3"}]],"columns":null})
        );
        for bad in [
            "F7",
            "F7@v2#page=0",
            "F7@v2#page=03",
            "F7@v2#x=1",
            "F7@v2#page=1#panel=A",
            "F7@v2#panel=",
            r"F7@v2#panel=a\n",
        ] {
            assert_eq!(
                Citation::parse_cli(bad, None).unwrap_err().code,
                Code::E6007,
                "{bad}"
            );
        }
        for bad in [
            "F7@v2#page=3#sample=a",
            "F7@v2#sample=a",
            "F7@v2#sample=a#page=x",
        ] {
            assert_eq!(
                Citation::parse_cli(bad, Some(&spec2())).unwrap_err().code,
                Code::E6007,
                "{bad}"
            );
        }
    }

    #[test]
    fn json_round_trip() {
        for v in [
            json!({"kind":"cells","version":"F7@v2","keys":[[{"t":"str","v":"TP53"}]],"columns":["padj"]}),
            json!({"kind":"whole_file","version":"F1@v1"}),
            json!({"kind":"page","version":"F3@v1","page":4}),
            json!({"kind":"panel","version":"F4@v1","panel":"B"}),
            json!({"kind":"quote","anything":[1,2]}),
        ] {
            assert_eq!(Citation::from_json(&v, "").unwrap().to_json(), v);
        }
        let e = Citation::from_json(
            &json!({"kind":"cells","version":"F7@v2","keys":[[{"t":"missing"}]],"columns":null}),
            "/c",
        )
        .unwrap_err();
        assert_eq!(
            (e.code, e.field.unwrap()),
            (Code::E6004, "/c/keys/0/0".to_owned())
        );
    }
}
