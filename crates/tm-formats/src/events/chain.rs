//! `chain.` events (11.16).

use super::{EventType, Form, TypeDef};

pub(crate) const TYPES: &[TypeDef] = &[TypeDef {
    ty: EventType::ChainTornTailTruncated,
    name: "chain.torn_tail_truncated",
    tv: 1,
    keys: &[
        ("segment", Form::Int),
        ("offset", Form::Int),
        ("bytes", Form::Int),
    ],
}];
