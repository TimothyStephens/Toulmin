//! `audit.` events (11.16).

use super::{EventType, Form, TypeDef};

pub(crate) const TYPES: &[TypeDef] = &[TypeDef {
    ty: EventType::AuditCompleted,
    name: "audit.completed",
    tv: 1,
    keys: &[
        ("report_hash", Form::Id),
        ("since", Form::Null(&Form::Id)),
        (
            "counts",
            Form::Object(&[
                ("files_rehashed", Form::Int),
                ("mismatches", Form::Int),
                ("verdicts_rerun", Form::Int),
                ("verdicts_changed", Form::Int),
                ("open_flags", Form::Int),
            ]),
        ),
    ],
}];
