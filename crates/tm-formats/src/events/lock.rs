//! `lock.` events (11.16).

use super::{EventType, Form, TypeDef};

pub(crate) const TYPES: &[TypeDef] = &[TypeDef {
    ty: EventType::LockTakenOver,
    name: "lock.taken_over",
    tv: 1,
    keys: &[
        (
            "previous",
            Form::Object(&[
                ("host", Form::NonEmptyStr),
                ("pid", Form::Int),
                ("epoch", Form::Int),
                ("heartbeat", Form::At),
            ]),
        ),
        ("epoch", Form::Int),
    ],
}];
