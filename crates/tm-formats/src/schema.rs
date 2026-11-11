//! Helpers shared by the validators: member access with JSON Pointer
//! errors. Each validator decides its own codes.

use serde_json::{Map, Value};

use crate::errors::{Code, Error, ptr};

/// Codes a validator reports for an absent or unknown member and for a
/// member of the wrong JSON type.
#[derive(Clone, Copy)]
pub(crate) struct Codes {
    pub missing: Code,
    pub wrong_type: Code,
}

pub(crate) fn object<'a>(
    v: &'a Value,
    at: &str,
    c: Codes,
    what: &str,
) -> Result<&'a Map<String, Value>, Error> {
    v.as_object()
        .ok_or_else(|| Error::at(c.wrong_type, at, format!("{what} must be an object")))
}

/// Check that `m` holds every key of `required` and nothing outside
/// `required` and `optional`. Absent keys are reported first, in the order
/// listed, then unknown keys in canonical order.
pub(crate) fn keys(
    m: &Map<String, Value>,
    at: &str,
    required: &[&str],
    optional: &[&str],
    c: Codes,
) -> Result<(), Error> {
    for k in required {
        if !m.contains_key(*k) {
            return Err(Error::at(
                c.missing,
                ptr(at, k),
                format!("required member `{k}` is absent"),
            ));
        }
    }
    let mut extra: Vec<&String> = m
        .keys()
        .filter(|k| !required.contains(&k.as_str()) && !optional.contains(&k.as_str()))
        .collect();
    extra.sort_by(|a, b| crate::canonical::utf16_cmp(a, b));
    if let Some(k) = extra.first() {
        return Err(Error::at(
            c.missing,
            ptr(at, k),
            format!("member `{k}` is not defined here"),
        ));
    }
    Ok(())
}

pub(crate) fn string<'a>(
    m: &'a Map<String, Value>,
    key: &str,
    at: &str,
    c: Codes,
) -> Result<&'a str, Error> {
    m.get(key).and_then(Value::as_str).ok_or_else(|| {
        Error::at(
            c.wrong_type,
            ptr(at, key),
            format!("`{key}` must be a string"),
        )
    })
}

pub(crate) fn nonempty_string<'a>(
    m: &'a Map<String, Value>,
    key: &str,
    at: &str,
    c: Codes,
) -> Result<&'a str, Error> {
    let s = string(m, key, at, c)?;
    if s.is_empty() {
        return Err(Error::at(
            c.wrong_type,
            ptr(at, key),
            format!("`{key}` must not be empty"),
        ));
    }
    Ok(s)
}

/// A member that must be one of `allowed`; anything else (including a
/// non-string) is `c.missing`, the "choice not among its listed values" rule.
pub(crate) fn choice<'a>(
    m: &'a Map<String, Value>,
    key: &str,
    at: &str,
    allowed: &[&str],
    c: Codes,
) -> Result<&'a str, Error> {
    match m.get(key).and_then(Value::as_str) {
        Some(s) if allowed.contains(&s) => Ok(s),
        Some(s) => Err(Error::at(
            c.missing,
            ptr(at, key),
            format!("`{key}` is {s:?}; expected one of {allowed:?}"),
        )),
        None => Err(Error::at(
            c.wrong_type,
            ptr(at, key),
            format!("`{key}` must be a string"),
        )),
    }
}

pub(crate) fn boolean(
    m: &Map<String, Value>,
    key: &str,
    at: &str,
    c: Codes,
) -> Result<bool, Error> {
    m.get(key).and_then(Value::as_bool).ok_or_else(|| {
        Error::at(
            c.wrong_type,
            ptr(at, key),
            format!("`{key}` must be a boolean"),
        )
    })
}

pub(crate) fn integer(
    m: &Map<String, Value>,
    key: &str,
    at: &str,
    min: u64,
    c: Codes,
) -> Result<u64, Error> {
    match m.get(key).and_then(Value::as_u64) {
        Some(n) if n >= min => Ok(n),
        _ => Err(Error::at(
            c.wrong_type,
            ptr(at, key),
            format!("`{key}` must be an integer of at least {min}"),
        )),
    }
}

pub(crate) fn array<'a>(
    m: &'a Map<String, Value>,
    key: &str,
    at: &str,
    nonempty: bool,
    c: Codes,
) -> Result<&'a Vec<Value>, Error> {
    match m.get(key).and_then(Value::as_array) {
        Some(a) if !nonempty || !a.is_empty() => Ok(a),
        Some(_) => Err(Error::at(
            c.wrong_type,
            ptr(at, key),
            format!("`{key}` must not be empty"),
        )),
        None => Err(Error::at(
            c.wrong_type,
            ptr(at, key),
            format!("`{key}` must be a list"),
        )),
    }
}

/// A list of strings, optionally non-empty and with distinct items.
pub(crate) fn string_list(
    m: &Map<String, Value>,
    key: &str,
    at: &str,
    nonempty: bool,
    distinct: bool,
    c: Codes,
) -> Result<Vec<String>, Error> {
    let a = array(m, key, at, nonempty, c)?;
    let base = ptr(at, key);
    let mut out: Vec<String> = Vec::with_capacity(a.len());
    for (i, v) in a.iter().enumerate() {
        let Some(s) = v.as_str() else {
            return Err(Error::at(
                c.wrong_type,
                ptr(&base, i),
                "list items must be strings",
            ));
        };
        if distinct && out.iter().any(|o| o == s) {
            return Err(Error::at(
                c.missing,
                ptr(&base, i),
                format!("{s:?} appears twice"),
            ));
        }
        out.push(s.to_owned());
    }
    Ok(out)
}
