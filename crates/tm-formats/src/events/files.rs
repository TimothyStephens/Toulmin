//! `file.` events (11.16). `file.tracked`, `file.retired`, and
//! `file.hashing_finished` are reserved and not declared in gate A.

use super::{EventType, Form, TypeDef};

pub(crate) const TYPES: &[TypeDef] = &[
    TypeDef {
        ty: EventType::FileRegistered,
        name: "file.registered",
        tv: 1,
        keys: &[
            ("file", Form::File),
            ("kind", Form::Choice(&["input"])),
            ("path", Form::Str),
            ("title", Form::Any),
            ("description", Form::Any),
            ("source", Form::Any),
        ],
    },
    TypeDef {
        ty: EventType::FileVersionAdded,
        name: "file.version_added",
        tv: 1,
        keys: &[
            ("file", Form::File),
            ("version", Form::Version),
            ("fingerprint", Form::Id),
            ("origin", Form::Origin),
            ("size", Form::Any),
            ("snapshot", Form::Snapshot),
            ("content_label", Form::Any),
            ("manifest", Form::Null(&Form::Manifest)),
        ],
    },
    TypeDef {
        ty: EventType::FileChecksumChecked,
        name: "file.checksum_checked",
        tv: 1,
        keys: &[
            ("file", Form::File),
            ("version", Form::Version),
            ("algorithm", Form::Any),
            ("matched", Form::Any),
            ("mismatches", Form::List(&Form::Str)),
        ],
    },
    TypeDef {
        ty: EventType::FileParseSpecSet,
        name: "file.parse_spec_set",
        tv: 1,
        keys: &[
            ("file", Form::File),
            ("spec", Form::Spec),
            ("spec_fingerprint", Form::Id),
        ],
    },
    TypeDef {
        ty: EventType::FileKept,
        name: "file.kept",
        tv: 1,
        keys: &[
            ("version", Form::Version),
            ("object", Form::Id),
            ("compressed_size", Form::Any),
            ("reason", Form::Choice(&["promotion", "owner"])),
            ("fingerprint_only", Form::Any),
        ],
    },
    TypeDef {
        ty: EventType::FileMarkedRemovable,
        name: "file.marked_removable",
        tv: 1,
        keys: &[("file", Form::File)],
    },
    TypeDef {
        ty: EventType::FileRestored,
        name: "file.restored",
        tv: 1,
        keys: &[("file", Form::File)],
    },
    TypeDef {
        ty: EventType::FileRenamed,
        name: "file.renamed",
        tv: 1,
        keys: &[("file", Form::File), ("from", Form::Str), ("to", Form::Str)],
    },
    TypeDef {
        ty: EventType::FilePanelsSet,
        name: "file.panels_set",
        tv: 1,
        keys: &[
            ("version", Form::Version),
            ("panels", Form::List(&Form::Str)),
        ],
    },
];
