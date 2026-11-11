//! `report.` events (11.16). `report` is the `T<n>` handle `report.add`
//! allocates; `path` the report's repository-relative path.

use super::{EventType, Form, TypeDef};

pub(crate) const TYPES: &[TypeDef] = &[TypeDef {
    ty: EventType::ReportRegistered,
    name: "report.registered",
    tv: 1,
    keys: &[("report", Form::Report), ("path", Form::Str)],
}];
