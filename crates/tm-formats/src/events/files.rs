//! `file.` events (11.16). `file.tracked`, `file.retired`, and
//! `file.hashing_finished` are reserved and not declared in gate A.
//! `file.version_added` has two versions: `tv` 2 adds
//! `decompressed_fingerprint`; a reader accepts both and a writer writes 2.

use super::{EventType, Form, TypeDef};

pub(crate) const TYPES: &[TypeDef] = &[
    TypeDef {
        ty: EventType::FileRegistered,
        name: "file.registered",
        tv: 1,
        keys: &[
            ("file", Form::File),
            ("kind", Form::Choice(&["input", "output"])),
            ("path", Form::NonEmptyStr),
            ("title", Form::NonEmptyStr),
            ("description", Form::NonEmptyStr),
            ("source", Form::Null(&Form::Str)),
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
            ("size", Form::Int),
            ("snapshot", Form::Snapshot),
            (
                "content_label",
                Form::Null(&Form::Choice(&[
                    "content_changed",
                    "carried_forward",
                    "metadata_only",
                ])),
            ),
            ("manifest", Form::Null(&Form::Manifest)),
        ],
    },
    TypeDef {
        ty: EventType::FileVersionAdded,
        name: "file.version_added",
        tv: 2,
        keys: &[
            ("file", Form::File),
            ("version", Form::Version),
            ("fingerprint", Form::Id),
            ("origin", Form::Origin),
            ("size", Form::Int),
            ("snapshot", Form::Snapshot),
            (
                "content_label",
                Form::Null(&Form::Choice(&[
                    "content_changed",
                    "carried_forward",
                    "metadata_only",
                ])),
            ),
            ("manifest", Form::Null(&Form::Manifest)),
            ("decompressed_fingerprint", Form::Null(&Form::Id)),
        ],
    },
    TypeDef {
        ty: EventType::FileChecksumChecked,
        name: "file.checksum_checked",
        tv: 1,
        keys: &[
            ("file", Form::File),
            ("version", Form::Version),
            ("algorithm", Form::Choice(&["md5", "sha256"])),
            ("matched", Form::Bool),
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
            ("compressed_size", Form::Int),
            ("reason", Form::Choice(&["promotion", "owner"])),
            ("fingerprint_only", Form::Bool),
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
        keys: &[
            ("file", Form::File),
            ("from", Form::NonEmptyStr),
            ("to", Form::NonEmptyStr),
        ],
    },
    TypeDef {
        ty: EventType::FilePanelsSet,
        name: "file.panels_set",
        tv: 1,
        keys: &[
            ("version", Form::Version),
            ("panels", Form::List(&Form::NonEmptyStr)),
        ],
    },
];
