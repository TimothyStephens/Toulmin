//! Toulmin shared formats (design section 11, items 11 to 25).
//!
//! Pure: no I/O except the optional `TM_ERROR_LOG` append in
//! [`errors::Error::new`]. Every other crate takes its formats from here.

// The 11.12 error shape is fixed and errors are the cold path; boxing it
// would buy nothing measurable.
#![allow(clippy::result_large_err)]

pub mod assertion;
pub mod canonical;
mod catalog;
pub mod citation;
pub mod envelope;
pub mod errors;
pub mod events;
pub mod expr;
pub mod id;
pub mod parse_spec;
pub(crate) mod schema;
pub mod sort_order;
pub mod steps;
pub mod typed_value;
pub mod version;

pub use errors::{Code, Error};
