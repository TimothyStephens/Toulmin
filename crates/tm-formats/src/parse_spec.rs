//! Parse spec (design 11.7, item 16): schema, validation, fingerprint.
//!
//! Codes: `E3103` for a key name the columns list lacks or a column the
//! file's header lacks, `E3104` for an encoding other than `utf-8`, `E3105`
//! for any other schema violation.

use serde_json::{Map, Value, json};

use crate::errors::{Code, Error, ptr};
use crate::id::{EventId, id_of};
use crate::schema::{self, Codes};
use crate::typed_value::SpecType;
use crate::version;

const CODES: Codes = Codes {
    missing: Code::E3105,
    wrong_type: Code::E3105,
};

/// The file format a spec reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Comma- or other-delimited text.
    Csv,
    /// Tab-delimited text.
    Tsv,
    /// Apache Parquet.
    Parquet,
}

/// Whether `nan` or `inf` tokens become `missing` or a value (11.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecialAs {
    /// Read as `missing`.
    Missing,
    /// Read as `nan` or `inf`.
    Value,
}

/// One column of a spec.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Column {
    /// The column name.
    pub name: String,
    /// The column type.
    pub ty: SpecType,
    /// Tokens read as `missing`.
    pub missing: Vec<String>,
    /// The scale of a `dec` column; `None` for every other type.
    pub scale: Option<u8>,
}

/// A validated 11.7 parse spec. `raw` is the object as given, which is what
/// the fingerprint covers.
#[derive(Debug, Clone, PartialEq)]
pub struct ParseSpec {
    /// The file format.
    pub format: Format,
    /// The delimiter (`csv` and `tsv`).
    pub delimiter: Option<char>,
    /// Whether the first line is a header (`csv` and `tsv`).
    pub header: Option<bool>,
    /// The columns in file order.
    pub columns: Vec<Column>,
    /// The key column names in key order.
    pub key: Vec<String>,
    /// How NaN tokens are read.
    pub nan: SpecialAs,
    /// How infinity tokens are read.
    pub inf: SpecialAs,
    raw: Value,
}

fn special(m: &Map<String, Value>, key: &str) -> Result<SpecialAs, Error> {
    Ok(
        match schema::choice(m, key, "", &["missing", "value"], CODES)? {
            "missing" => SpecialAs::Missing,
            _ => SpecialAs::Value,
        },
    )
}

impl ParseSpec {
    /// Validate an 11.7 object.
    ///
    /// # Errors
    /// `E3103`, `E3104`, or `E3105` with `field` at the offending member.
    #[allow(clippy::too_many_lines, reason = "one check per 11.7 member, in order")]
    pub fn from_json(v: &Value) -> Result<ParseSpec, Error> {
        let m = schema::object(v, "", CODES, "a parse spec")?;
        schema::keys(
            m,
            "",
            &[
                "spec",
                "format",
                "delimiter",
                "header",
                "columns",
                "key",
                "nan",
                "inf",
                "encoding",
            ],
            &[],
            CODES,
        )?;
        if m["spec"].as_u64() != Some(version::PARSE_SPEC) {
            return Err(Error::at(
                Code::E3105,
                "/spec",
                "unknown parse spec version",
            ));
        }
        let format = match schema::choice(m, "format", "", &["csv", "tsv", "parquet"], CODES)? {
            "csv" => Format::Csv,
            "tsv" => Format::Tsv,
            _ => Format::Parquet,
        };
        let (delimiter, header) = if format == Format::Parquet {
            for k in ["delimiter", "header"] {
                if !m[k].is_null() {
                    return Err(Error::at(
                        Code::E3105,
                        ptr("", k),
                        format!("`{k}` is null for parquet"),
                    ));
                }
            }
            (None, None)
        } else {
            let d = schema::string(m, "delimiter", "", CODES)?;
            let mut chars = d.chars();
            let (Some(c), None) = (chars.next(), chars.next()) else {
                return Err(Error::at(
                    Code::E3105,
                    "/delimiter",
                    "the delimiter is one character",
                ));
            };
            (Some(c), Some(schema::boolean(m, "header", "", CODES)?))
        };
        let cols = schema::array(m, "columns", "", true, CODES)?;
        let mut columns: Vec<Column> = Vec::with_capacity(cols.len());
        for (i, c) in cols.iter().enumerate() {
            let at = ptr("/columns", i);
            let cm = schema::object(c, &at, CODES, "a column")?;
            let ty_name = cm.get("type").and_then(Value::as_str);
            let is_dec = ty_name == Some("dec");
            let required: &[&str] = if is_dec {
                &["name", "type", "missing", "scale"]
            } else {
                &["name", "type", "missing"]
            };
            schema::keys(cm, &at, required, &[], CODES)?;
            let name = schema::nonempty_string(cm, "name", &at, CODES)?;
            if columns.iter().any(|c| c.name == name) {
                return Err(Error::at(
                    Code::E3105,
                    ptr(&at, "name"),
                    format!("column {name:?} appears twice"),
                ));
            }
            let ty = SpecType::parse(schema::string(cm, "type", &at, CODES)?)
                .ok_or_else(|| Error::at(Code::E3105, ptr(&at, "type"), "unknown column type"))?;
            let missing = schema::string_list(cm, "missing", &at, false, false, CODES)?;
            let scale =
                if is_dec {
                    let s = schema::integer(cm, "scale", &at, 0, CODES)?;
                    Some(u8::try_from(s).ok().filter(|s| *s <= 38).ok_or_else(|| {
                        Error::at(Code::E3105, ptr(&at, "scale"), "scale is 0 to 38")
                    })?)
                } else {
                    None
                };
            columns.push(Column {
                name: name.to_owned(),
                ty,
                missing,
                scale,
            });
        }
        let key = schema::string_list(m, "key", "", true, true, CODES)?;
        for (i, k) in key.iter().enumerate() {
            if !columns.iter().any(|c| &c.name == k) {
                return Err(Error::at(
                    Code::E3103,
                    ptr("/key", i),
                    format!("key column {k:?} is not a column"),
                ));
            }
        }
        let nan = special(m, "nan")?;
        let inf = special(m, "inf")?;
        let encoding = schema::string(m, "encoding", "", CODES)?;
        if encoding != "utf-8" {
            return Err(Error::at(
                Code::E3104,
                "/encoding",
                format!("unsupported encoding {encoding:?}"),
            ));
        }
        // A spec is a hashed record; refuse anything canonical JSON refuses.
        id_of(v)?;
        Ok(ParseSpec {
            format,
            delimiter,
            header,
            columns,
            key,
            nan,
            inf,
            raw: v.clone(),
        })
    }

    /// Check the spec's columns against a file's header.
    ///
    /// # Errors
    /// `E3103` at the first column the header does not have.
    pub fn check_header(&self, header: &[&str]) -> Result<(), Error> {
        for (i, c) in self.columns.iter().enumerate() {
            if !header.contains(&c.name.as_str()) {
                return Err(Error::at(
                    Code::E3103,
                    ptr(&ptr("/columns", i), "name"),
                    format!("column {:?} is not in the header", c.name),
                ));
            }
        }
        Ok(())
    }

    /// The fingerprint: the ID of the spec's canonical JSON.
    #[must_use]
    pub fn fingerprint(&self) -> EventId {
        // `from_json` already checked the spec canonicalizes.
        id_of(&self.raw).unwrap_or_else(|_| crate::id::derive_id(b""))
    }

    /// The spec as given.
    #[must_use]
    pub fn to_json(&self) -> &Value {
        &self.raw
    }

    /// The column called `name`.
    #[must_use]
    pub fn column(&self, name: &str) -> Option<&Column> {
        self.columns.iter().find(|c| c.name == name)
    }
}

/// A minimal example spec, used by tests and documentation.
#[must_use]
pub fn example() -> Value {
    json!({
        "spec": 1, "format": "tsv", "delimiter": "\t", "header": true,
        "columns": [
            {"name": "gene_id", "type": "str", "missing": []},
            {"name": "padj", "type": "dec", "missing": ["NA"], "scale": 5}
        ],
        "key": ["gene_id"], "nan": "missing", "inf": "missing", "encoding": "utf-8"
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_and_fingerprint() {
        let s = ParseSpec::from_json(&example()).unwrap();
        assert_eq!(s.key, vec!["gene_id"]);
        assert_eq!(s.column("padj").unwrap().scale, Some(5));
        assert!(crate::id::is_sha256(s.fingerprint().as_str()));
        assert!(s.check_header(&["gene_id", "padj", "baseMean"]).is_ok());
        let e = s.check_header(&["gene_id"]).unwrap_err();
        assert_eq!(
            (e.code, e.field.as_deref()),
            (Code::E3103, Some("/columns/1/name"))
        );
    }

    #[test]
    fn refusals() {
        let mut v = example();
        v["encoding"] = json!("latin-1");
        assert_eq!(ParseSpec::from_json(&v).unwrap_err().code, Code::E3104);
        let mut v = example();
        v["key"] = json!(["nope"]);
        let e = ParseSpec::from_json(&v).unwrap_err();
        assert_eq!((e.code, e.field.as_deref()), (Code::E3103, Some("/key/0")));
        let mut v = example();
        v["columns"][1].as_object_mut().unwrap().remove("scale");
        let e = ParseSpec::from_json(&v).unwrap_err();
        assert_eq!(
            (e.code, e.field.as_deref()),
            (Code::E3105, Some("/columns/1/scale"))
        );
        let mut v = example();
        v["columns"][0]["scale"] = json!(2);
        let e = ParseSpec::from_json(&v).unwrap_err();
        assert_eq!(
            (e.code, e.field.as_deref()),
            (Code::E3105, Some("/columns/0/scale"))
        );
    }
}
