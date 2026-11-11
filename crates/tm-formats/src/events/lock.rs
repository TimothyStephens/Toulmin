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
                ("host", Form::Any),
                ("pid", Form::Any),
                ("epoch", Form::Any),
                ("heartbeat", Form::Any),
            ]),
        ),
        ("epoch", Form::Any),
    ],
}];
