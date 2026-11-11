//! RFC 8785 (JCS) canonical JSON over `serde_json::Value` (design 11.1,
//! item 11).
//!
//! [`parse`] is a strict I-JSON reader (RFC 7493): any syntax error,
//! duplicate member name, lone surrogate, or `NaN`/`Infinity` token is
//! `E1003`, and a number with a fraction or exponent, a minus zero, or an
//! integer outside plus or minus 2^53 is `E1001`. [`canonical`] serializes
//! a value with members sorted by UTF-16 code units and refuses any number
//! that is not such an integer with `E1001`.
//!
//! Error precedence matches the Python reference
//! (`reference/formats/canonical_json.py`): floats and syntax errors are
//! reported where the scanner meets them, a duplicate name when its object
//! closes, and a lone surrogate only once the whole document has parsed.

use std::fmt::Write as _;

use serde_json::{Map, Number, Value};

use crate::errors::{Code, Error};

/// The largest integer magnitude a hashed record may hold as a JSON number.
pub const INT_LIMIT: u64 = (1 << 53) - 1;

/// Parse UTF-8 bytes strictly. Invalid UTF-8 is `E1003`.
///
/// # Errors
/// `E1003` for anything that is not I-JSON, `E1001` for a float or an
/// out-of-range integer.
pub fn parse_bytes(data: &[u8]) -> Result<Value, Error> {
    let text = std::str::from_utf8(data)
        .map_err(|e| Error::new(Code::E1003, format!("not I-JSON: invalid UTF-8 ({e})")))?;
    parse(text)
}

/// Parse text strictly; see the module documentation.
///
/// # Errors
/// `E1003` for anything that is not I-JSON, `E1001` for a float or an
/// out-of-range integer.
pub fn parse(text: &str) -> Result<Value, Error> {
    let mut p = Parser {
        s: text.as_bytes(),
        i: 0,
        lone_surrogate: false,
        depth: 0,
    };
    p.ws();
    let v = p.value()?;
    p.ws();
    if p.i != p.s.len() {
        return Err(p.syntax("extra data after the value"));
    }
    if p.lone_surrogate {
        return Err(Error::new(Code::E1003, "not I-JSON: lone surrogate"));
    }
    Ok(v)
}

const MAX_DEPTH: usize = 512;

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
    lone_surrogate: bool,
    depth: usize,
}

impl Parser<'_> {
    fn syntax(&self, what: &str) -> Error {
        Error::new(
            Code::E1003,
            format!("not I-JSON: {what} at byte {}", self.i),
        )
    }

    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    fn ws(&mut self) {
        while let Some(b' ' | b'\t' | b'\n' | b'\r') = self.peek() {
            self.i += 1;
        }
    }

    fn eat(&mut self, lit: &[u8]) -> bool {
        if self.s[self.i..].starts_with(lit) {
            self.i += lit.len();
            true
        } else {
            false
        }
    }

    fn value(&mut self) -> Result<Value, Error> {
        match self.peek() {
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => Ok(Value::String(self.string()?)),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(b't') if self.eat(b"true") => Ok(Value::Bool(true)),
            Some(b'f') if self.eat(b"false") => Ok(Value::Bool(false)),
            Some(b'n') if self.eat(b"null") => Ok(Value::Null),
            _ => Err(self.syntax("expected a value")),
        }
    }

    fn enter(&mut self) -> Result<(), Error> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(self.syntax("nesting too deep"));
        }
        Ok(())
    }

    fn object(&mut self) -> Result<Value, Error> {
        self.enter()?;
        self.i += 1;
        let mut pairs: Vec<(String, Value)> = Vec::new();
        self.ws();
        if self.peek() == Some(b'}') {
            self.i += 1;
        } else {
            loop {
                self.ws();
                if self.peek() != Some(b'"') {
                    return Err(self.syntax("expected a member name"));
                }
                let k = self.string()?;
                self.ws();
                if self.peek() != Some(b':') {
                    return Err(self.syntax("expected ':'"));
                }
                self.i += 1;
                self.ws();
                let v = self.value()?;
                pairs.push((k, v));
                self.ws();
                match self.peek() {
                    Some(b',') => self.i += 1,
                    Some(b'}') => {
                        self.i += 1;
                        break;
                    }
                    _ => return Err(self.syntax("expected ',' or '}'")),
                }
            }
        }
        self.depth -= 1;
        let mut map = Map::new();
        for (k, v) in pairs {
            if map.contains_key(&k) {
                return Err(Error::new(
                    Code::E1003,
                    format!("not I-JSON: duplicate member name {k:?}"),
                ));
            }
            map.insert(k, v);
        }
        Ok(Value::Object(map))
    }

    fn array(&mut self) -> Result<Value, Error> {
        self.enter()?;
        self.i += 1;
        let mut items = Vec::new();
        self.ws();
        if self.peek() == Some(b']') {
            self.i += 1;
        } else {
            loop {
                self.ws();
                items.push(self.value()?);
                self.ws();
                match self.peek() {
                    Some(b',') => self.i += 1,
                    Some(b']') => {
                        self.i += 1;
                        break;
                    }
                    _ => return Err(self.syntax("expected ',' or ']'")),
                }
            }
        }
        self.depth -= 1;
        Ok(Value::Array(items))
    }

    fn hex4(&mut self) -> Result<u32, Error> {
        let Some(h) = self.s.get(self.i..self.i + 4) else {
            return Err(self.syntax("short \\u escape"));
        };
        let mut v = 0u32;
        for &b in h {
            let d = match b {
                b'0'..=b'9' => b - b'0',
                b'a'..=b'f' => b - b'a' + 10,
                b'A'..=b'F' => b - b'A' + 10,
                _ => return Err(self.syntax("bad \\u escape")),
            };
            v = v * 16 + u32::from(d);
        }
        self.i += 4;
        Ok(v)
    }

    fn string(&mut self) -> Result<String, Error> {
        self.i += 1;
        let mut out = String::new();
        loop {
            let start = self.i;
            while let Some(b) = self.peek() {
                if b == b'"' || b == b'\\' || b < 0x20 {
                    break;
                }
                self.i += 1;
            }
            // The input is a &str, so any run of non-special bytes is UTF-8.
            out.push_str(
                std::str::from_utf8(&self.s[start..self.i])
                    .map_err(|_| self.syntax("invalid UTF-8"))?,
            );
            match self.peek() {
                Some(b'"') => {
                    self.i += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.i += 1;
                    let Some(e) = self.peek() else {
                        return Err(self.syntax("unterminated string"));
                    };
                    self.i += 1;
                    match e {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let hi = self.hex4()?;
                            if (0xD800..0xDC00).contains(&hi) {
                                // A high surrogate pairs only with an
                                // immediately following \u low surrogate.
                                let save = self.i;
                                if self.eat(b"\\u") {
                                    let lo = self.hex4()?;
                                    if (0xDC00..0xE000).contains(&lo) {
                                        let c = 0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00);
                                        out.push(char::from_u32(c).unwrap_or('\u{fffd}'));
                                        continue;
                                    }
                                }
                                self.i = save;
                                self.lone_surrogate = true;
                                out.push('\u{fffd}');
                            } else if (0xDC00..0xE000).contains(&hi) {
                                self.lone_surrogate = true;
                                out.push('\u{fffd}');
                            } else {
                                out.push(char::from_u32(hi).unwrap_or('\u{fffd}'));
                            }
                        }
                        _ => return Err(self.syntax("bad escape")),
                    }
                }
                Some(_) => return Err(self.syntax("control character in string")),
                None => return Err(self.syntax("unterminated string")),
            }
        }
    }

    fn digits(&mut self) -> usize {
        let start = self.i;
        while let Some(b'0'..=b'9') = self.peek() {
            self.i += 1;
        }
        self.i - start
    }

    fn number(&mut self) -> Result<Value, Error> {
        let start = self.i;
        let neg = self.peek() == Some(b'-');
        if neg {
            self.i += 1;
        }
        match self.peek() {
            Some(b'0') => self.i += 1,
            Some(b'1'..=b'9') => {
                self.digits();
            }
            _ => return Err(self.syntax("bad number")),
        }
        let int_end = self.i;
        let mut float = false;
        if self.peek() == Some(b'.') {
            let save = self.i;
            self.i += 1;
            if self.digits() == 0 {
                self.i = save;
                return Err(self.syntax("bad number"));
            }
            float = true;
        }
        if let Some(b'e' | b'E') = self.peek() {
            let save = self.i;
            self.i += 1;
            if let Some(b'+' | b'-') = self.peek() {
                self.i += 1;
            }
            if self.digits() == 0 {
                self.i = save;
                return Err(self.syntax("bad number"));
            }
            float = true;
        }
        let text = std::str::from_utf8(&self.s[start..self.i]).unwrap_or("?");
        if float {
            return Err(Error::new(
                Code::E1001,
                format!("float in canonical JSON: {text}"),
            ));
        }
        let digits = std::str::from_utf8(&self.s[if neg { start + 1 } else { start }..int_end])
            .unwrap_or("?");
        if neg && digits == "0" {
            return Err(Error::new(Code::E1001, "minus zero in canonical JSON"));
        }
        let mag: Option<u64> = digits.parse().ok().filter(|m| *m <= INT_LIMIT);
        let Some(mag) = mag else {
            return Err(Error::new(
                Code::E1001,
                format!("integer outside plus or minus 2^53-1: {text}"),
            ));
        };
        let n = if neg {
            // mag <= 2^53-1 fits i64.
            Number::from(-i64::try_from(mag).unwrap_or(i64::MAX))
        } else {
            Number::from(mag)
        };
        Ok(Value::Number(n))
    }
}

fn number_text(n: &Number) -> Result<String, Error> {
    if let Some(u) = n.as_u64() {
        if u <= INT_LIMIT {
            return Ok(u.to_string());
        }
    } else if let Some(i) = n.as_i64() {
        if i.unsigned_abs() <= INT_LIMIT {
            return Ok(i.to_string());
        }
    } else {
        return Err(Error::new(
            Code::E1001,
            format!("float in canonical JSON: {n}"),
        ));
    }
    Err(Error::new(
        Code::E1001,
        format!("integer outside plus or minus 2^53: {n}"),
    ))
}

fn write_string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Compare member names by their UTF-16 code units (RFC 8785 3.2.3).
#[must_use]
pub fn utf16_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}

fn write_value(v: &Value, out: &mut String) -> Result<(), Error> {
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Number(n) => out.push_str(&number_text(n)?),
        Value::String(s) => write_string(s, out),
        Value::Array(a) => {
            out.push('[');
            for (i, item) in a.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_value(item, out)?;
            }
            out.push(']');
        }
        Value::Object(m) => {
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort_by(|a, b| utf16_cmp(a, b));
            out.push('{');
            for (i, k) in keys.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_string(k, out);
                out.push(':');
                write_value(&m[k], out)?;
            }
            out.push('}');
        }
    }
    Ok(())
}

/// The canonical text of a value.
///
/// # Errors
/// `E1001` if the value holds a float or an integer outside plus or minus
/// 2^53.
pub fn canonical(v: &Value) -> Result<String, Error> {
    let mut out = String::new();
    write_value(v, &mut out)?;
    Ok(out)
}

/// The canonical UTF-8 bytes of a value.
///
/// # Errors
/// As [`canonical`].
pub fn canonical_bytes(v: &Value) -> Result<Vec<u8>, Error> {
    canonical(v).map(String::into_bytes)
}

/// Parse text strictly and return its canonical text.
///
/// # Errors
/// As [`parse`] and [`canonical`].
pub fn canonicalize(text: &str) -> Result<String, Error> {
    canonical(&parse(text)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorts_by_utf16() {
        let v = parse(r#"{"b":1,"a":[true,null],"\u20ac":"x","\ud83d\ude00":2}"#).unwrap();
        assert_eq!(
            canonical(&v).unwrap(),
            "{\"a\":[true,null],\"b\":1,\"\u{20ac}\":\"x\",\"\u{1F600}\":2}"
        );
    }

    #[test]
    fn refuses_floats_and_bad_input() {
        assert_eq!(parse("1.0").unwrap_err().code, Code::E1001);
        assert_eq!(parse("-0").unwrap_err().code, Code::E1001);
        assert_eq!(parse("1e2").unwrap_err().code, Code::E1001);
        assert_eq!(parse("9007199254740992").unwrap_err().code, Code::E1001);
        assert_eq!(parse("-9007199254740992").unwrap_err().code, Code::E1001);
        assert!(parse("9007199254740991").is_ok());
        assert!(parse("-9007199254740991").is_ok());
        assert_eq!(parse(r#"{"a":1,"a":2}"#).unwrap_err().code, Code::E1003);
        assert_eq!(parse(r#""\ud800""#).unwrap_err().code, Code::E1003);
        assert_eq!(parse("NaN").unwrap_err().code, Code::E1003);
        assert_eq!(parse("[1,]").unwrap_err().code, Code::E1003);
        // A float inside wins over a duplicate name seen at object close,
        // and over a lone surrogate seen anywhere.
        assert_eq!(parse(r#"{"a":1,"a":2.5}"#).unwrap_err().code, Code::E1001);
        assert_eq!(parse(r#"["\udc00",1.5]"#).unwrap_err().code, Code::E1001);
    }

    #[test]
    fn escapes() {
        let v = Value::String("\u{1}\u{7f}\"\\/\n".into());
        assert_eq!(canonical(&v).unwrap(), "\"\\u0001\u{7f}\\\"\\\\/\\n\"");
    }

    #[test]
    fn refuses_constructed_floats() {
        let v = serde_json::json!({"a": 1.5});
        assert_eq!(canonical(&v).unwrap_err().code, Code::E1001);
    }
}
