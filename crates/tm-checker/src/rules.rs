//! General rules (design item 57, D12): required choices, `take` needs a
//! total order, the closed expression grammar, the schema checks against
//! the cited parse specs, and the rule version every verdict records.
//!
//! The structural rules are `tm-formats`' (11.8 and 11.9 are shared
//! formats, items 17 and 18); this module applies them in the Checker's
//! order before anything runs: the steps' shape, then the steps against
//! the cited tables' specs (columns, operand types, join cover), then the
//! assertion's shape, then the assertion against the final table's schema.

use std::collections::BTreeMap;

use serde_json::Value;

use tm_formats::assertion::Assertion;
use tm_formats::errors::Error;
use tm_formats::steps::{Steps, TableSchema};

/// The rule version recorded in `versions.rules`.
pub const RULES_VERSION: &str = "1";

/// Steps and an assertion that passed every check that needs no data.
#[derive(Debug, Clone)]
pub struct Validated {
    /// The steps; `None` for an assertion over a `cells` citation.
    pub steps: Option<Steps>,
    /// The assertion.
    pub assertion: Assertion,
    /// The final table's schema (11.18.2).
    pub final_schema: TableSchema,
}

/// Validate `steps` (or, when `None`, take `cited` as the final table's
/// schema) and `assertion` against the cited tables' schemas.
///
/// # Errors
/// The first 11.8 or 11.9 error, with its pointer.
pub fn validate(
    steps: Option<&Value>,
    assertion: &Value,
    schemas: &BTreeMap<String, TableSchema>,
    cited: Option<&TableSchema>,
) -> Result<Validated, Error> {
    let (steps, final_schema) = match steps {
        Some(v) => {
            let s = Steps::from_json(v)?;
            let schema = s.check(schemas)?;
            (Some(s), schema)
        }
        None => (
            None,
            cited.cloned().unwrap_or(TableSchema {
                columns: Vec::new(),
                key: Vec::new(),
            }),
        ),
    };
    let assertion = Assertion::from_json(assertion)?;
    assertion.check(&final_schema)?;
    Ok(Validated {
        steps,
        assertion,
        final_schema,
    })
}
