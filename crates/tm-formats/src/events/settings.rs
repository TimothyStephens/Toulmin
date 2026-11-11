//! `settings.` events (11.16).

use super::{EventType, Form, TypeDef};

pub(crate) const TYPES: &[TypeDef] = &[TypeDef {
    ty: EventType::SettingsChanged,
    name: "settings.changed",
    tv: 1,
    keys: &[
        ("key", Form::NonEmptyStr),
        ("value", Form::Null(&Form::Typed)),
        ("previous", Form::Null(&Form::Typed)),
    ],
}];
