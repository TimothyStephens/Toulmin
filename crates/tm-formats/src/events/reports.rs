//! `report.` events (11.16). The report handle's format is open until P6,
//! so `report` is any non-empty string.

use super::{EventType, Form, TypeDef};

pub(crate) const TYPES: &[TypeDef] = &[TypeDef {
    ty: EventType::ReportRegistered,
    name: "report.registered",
    tv: 1,
    keys: &[("report", Form::NonEmptyStr), ("path", Form::Str)],
}];
