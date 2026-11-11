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
            ("sentence", Form::NonEmptyStr),
            ("report", Form::Report),
            ("cited", Form::List(&Form::Version)),
            ("citations", Form::List(&Form::Citation)),
            ("steps", Form::Null(&Form::Steps)),
            ("assertion", Form::Null(&Form::Assertion)),
            ("qualifiers", Form::Null(&Form::Str)),
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
                Form::Object(&[
                    ("outcome", Form::Choice(OUTCOMES)),
                    ("detail", Form::Null(&Form::Str)),
                ]),
            ),
            (
                "assertion",
                Form::Null(&Form::Object(&[
                    ("outcome", Form::Choice(OUTCOMES)),
                    // The one open form in gate A: item 18 decides it at P4.
                    ("observed", Form::Any),
                    ("expected", Form::Expected),
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
                    ("rows_out", Form::Int),
                    ("description", Form::NonEmptyStr),
                ])),
            ),
            ("final_rows_hash", Form::Id),
            ("key_map_hash", Form::Id),
            ("key_map", Form::List(&Form::List(&Form::Typed))),
            ("sql", Form::Str),
            (
                "versions",
                Form::Object(&[
                    ("parse_specs", Form::SpecMap),
                    ("translator", Form::NonEmptyStr),
                    ("duckdb", Form::NonEmptyStr),
                    ("extensions", Form::List(&Form::Str)),
                    ("rules", Form::NonEmptyStr),
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
            ("sentence", Form::NonEmptyStr),
            ("cited", Form::List(&Form::Version)),
            ("citations", Form::List(&Form::Citation)),
            ("steps", Form::Null(&Form::Steps)),
            ("assertion", Form::Null(&Form::Assertion)),
            ("qualifiers", Form::Null(&Form::Str)),
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
            ("note", Form::Null(&Form::Str)),
            ("report_fingerprint", Form::Id),
        ],
    },
    TypeDef {
        ty: EventType::ClaimRetracted,
        name: "claim.retracted",
        tv: 1,
        keys: &[("ref", Form::Claim), ("reason", Form::NonEmptyStr)],
    },
    TypeDef {
        ty: EventType::ClaimSuperseded,
        name: "claim.superseded",
        tv: 1,
        keys: &[("ref", Form::Claim), ("replacement", Form::Claim)],
    },
];
