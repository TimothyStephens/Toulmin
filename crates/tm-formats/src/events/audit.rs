//! `audit.` events (11.16).

use super::{EventType, Form, TypeDef};

pub(crate) const TYPES: &[TypeDef] = &[TypeDef {
    ty: EventType::AuditCompleted,
    name: "audit.completed",
    tv: 1,
    keys: &[
        ("report_hash", Form::Any),
        ("since", Form::Any),
        (
            "counts",
            Form::Object(&[
                ("files_rehashed", Form::Any),
                ("mismatches", Form::Any),
                ("verdicts_rerun", Form::Any),
                ("verdicts_changed", Form::Any),
                ("open_flags", Form::Any),
            ]),
        ),
    ],
}];
