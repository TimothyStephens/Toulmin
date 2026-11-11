//! `claim.` events (11.16).

use super::{EventType, Form, TypeDef};

const OUTCOMES: &[&str] = &["holds", "fails", "not_checkable", "error"];

pub(crate) const TYPES: &[TypeDef] = &[
    TypeDef {
        ty: EventType::ClaimAdded,
        name: "claim.added",
        tv: 1,
        keys: &[
            ("ref", Form::Claim),
            (
                "kind",
                Form::Choice(&["quantitative", "qualitative", "mixed"]),
            ),
            ("sentence", Form::Any),
            ("report", Form::NonEmptyStr),
            ("cited", Form::List(&Form::Version)),
            ("citations", Form::List(&Form::Citation)),
            ("steps", Form::Null(&Form::Steps)),
            ("assertion", Form::Null(&Form::Assertion)),
            ("qualifiers", Form::Any),
            ("supersedes", Form::Null(&Form::Claim)),
        ],
    },
    TypeDef {
        ty: EventType::ClaimChecked,
        name: "claim.checked",
        tv: 1,
        keys: &[
            ("ref", Form::Claim),
            ("revision", Form::Id),
            ("against", Form::List(&Form::Version)),
            (
                "evidence",
                Form::Object(&[("outcome", Form::Choice(OUTCOMES)), ("detail", Form::Any)]),
            ),
            (
                "assertion",
                Form::Null(&Form::Object(&[
                    ("outcome", Form::Choice(OUTCOMES)),
                    ("observed", Form::Any),
                    ("expected", Form::Any),
                ])),
            ),
            (
                "steps",
                Form::List(&Form::Object(&[
                    (
                        "op",
                        Form::Choice(&[
                            "filter", "select", "sort", "take", "rank", "derive", "group", "join",
                        ]),
                    ),
                    ("rows_out", Form::Any),
                    ("description", Form::Any),
                ])),
            ),
            ("final_rows_hash", Form::Id),
            ("key_map_hash", Form::Id),
            ("key_map", Form::List(&Form::Any)),
            ("sql", Form::Any),
            (
                "versions",
                Form::Object(&[
                    ("parse_specs", Form::Any),
                    ("translator", Form::Any),
                    ("duckdb", Form::Any),
                    ("extensions", Form::Any),
                    ("rules", Form::Any),
                    ("build", Form::Choice(&["musl", "glibc"])),
                ]),
            ),
            (
                "error",
                Form::Null(&Form::Object(&[
                    ("code", Form::Str),
                    ("message", Form::Str),
                ])),
            ),
        ],
    },
    TypeDef {
        ty: EventType::ClaimRevised,
        name: "claim.revised",
        tv: 1,
        keys: &[
            ("ref", Form::Claim),
            ("sentence", Form::Any),
            ("cited", Form::List(&Form::Version)),
            ("citations", Form::List(&Form::Citation)),
            ("steps", Form::Null(&Form::Steps)),
            ("assertion", Form::Null(&Form::Assertion)),
            ("qualifiers", Form::Any),
            ("reason", Form::Choice(&["update_text"])),
        ],
    },
    TypeDef {
        ty: EventType::ClaimReviewed,
        name: "claim.reviewed",
        tv: 1,
        keys: &[
            ("ref", Form::Claim),
            ("revision", Form::Id),
            (
                "outcome",
                Form::Choice(&["confirmed", "disputed", "question"]),
            ),
            ("note", Form::Any),
            ("report_fingerprint", Form::Any),
        ],
    },
    TypeDef {
        ty: EventType::ClaimRetracted,
        name: "claim.retracted",
        tv: 1,
        keys: &[("ref", Form::Claim), ("reason", Form::Any)],
    },
    TypeDef {
        ty: EventType::ClaimSuperseded,
        name: "claim.superseded",
        tv: 1,
        keys: &[("ref", Form::Claim), ("replacement", Form::Claim)],
    },
];
