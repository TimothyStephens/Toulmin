//! `project.` events (11.16).

use super::{EventType, Form, TypeDef};

pub(crate) const TYPES: &[TypeDef] = &[TypeDef {
    ty: EventType::ProjectCreated,
    name: "project.created",
    tv: 1,
    keys: &[
        ("project_id", Form::NonEmptyStr),
        ("owner_id", Form::NonEmptyStr),
        ("record_format", Form::Any),
        ("repo_root_hash", Form::Any),
    ],
}];
