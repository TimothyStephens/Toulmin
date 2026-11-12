//! Item 52, on `fixtures/` (item 82a): registration; a spec with each
//! type; a duplicate key; each equivalence rule; each allowed and refused
//! 6.3 transition; both checksum forms; page counts and panels; the parse
//! cache; the fold. The fixture's specs, steps, assertions, and citations
//! are also checked against `tm-formats`, so the ten claims are well formed
//! before P8 runs them.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_lines,
    reason = "a test harness: panics report setup failures"
)]

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use arrow::datatypes::{DataType, TimeUnit};
use serde_json::{Value, json};
use tm_duckdb::EngineSettings;
use tm_files::attributes::{Action, Kept, State};
use tm_files::checksum::{Algorithm, compare, parse_checksums};
use tm_files::identity::{Identity, Level, Registration, check_input_path};
use tm_files::parse::{complete_spec, key_tuples, parse, typed_rows};
use tm_files::parse_cache::ParseCache;
use tm_files::version::{
    Candidate, ContentLabel, Equivalence, KeepReason, Origin, Version, decompressed_fingerprint,
    equivalence, same_table_by_key,
};
use tm_files::{Code, Error};
use tm_formats::assertion::Assertion;
use tm_formats::citation::Citation;
use tm_formats::envelope::{self, Actor, ActorKind, Line, Source, SourceKind};
use tm_formats::events::{EventType, KnownEvent};
use tm_formats::id::{EventId, FileHandle, VersionHandle, genesis};
use tm_formats::parse_spec::ParseSpec;
use tm_formats::steps::{Steps, TableSchema};
use tm_formats::typed_value::TypedValue;
use tm_fsfacts::hash::HashSettings;
use tm_index::{Index, Param};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn read_json(path: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(path).expect("read")).expect("json")
}

fn spec(name: &str) -> ParseSpec {
    ParseSpec::from_json(&read_json(
        &fixtures().join("specs").join(format!("{name}.json")),
    ))
    .expect("spec")
}

fn settings(dir: &Path) -> EngineSettings {
    EngineSettings {
        memory_limit_mib: 256,
        temp_directory: dir.to_string_lossy().into_owned(),
        call_seconds: 30,
    }
}

fn registration(title: &str) -> Registration {
    Registration {
        title: title.into(),
        description: "a fixture file".into(),
        source: Some("invented".into()),
    }
}

fn dec(s: &str) -> TypedValue {
    TypedValue::from_text(s, tm_formats::typed_value::SpecType::Dec).unwrap()
}

fn s(v: &str) -> TypedValue {
    TypedValue::str_nfc(v)
}

// ------------------------------------------------------------------ fixture

#[test]
fn fixture_specs_steps_assertions_and_citations_are_well_formed() {
    let root = fixtures();
    let specs: BTreeMap<&str, ParseSpec> = [
        ("de", spec("de")),
        ("samples", spec("samples")),
        ("counts", spec("counts")),
    ]
    .into_iter()
    .collect();
    let by_handle: BTreeMap<&str, &ParseSpec> = [
        ("F1", &specs["de"]),
        ("F2", &specs["samples"]),
        ("F3", &specs["counts"]),
    ]
    .into_iter()
    .collect();
    assert_eq!(
        specs["samples"].key,
        ["condition", "batch"],
        "claim 10's composite key"
    );
    let expected = read_json(&root.join("expected.json"));
    for n in 1..=10 {
        let dir = root.join("claims").join(n.to_string());
        let args: Vec<String> = fs::read_to_string(dir.join("args.txt"))
            .expect("args.txt")
            .lines()
            .map(str::to_owned)
            .collect();
        assert_eq!(args[0], "--report", "claim {n}");
        assert!(args.contains(&"--kind".to_owned()), "claim {n}");
        assert!(
            expected["claims"][n.to_string()].is_null(),
            "claim {n}: a stub until P8"
        );
        // Citations parse with the cited table's spec.
        let cited: Vec<&String> = args
            .windows(2)
            .filter(|w| w[0] == "--cited")
            .map(|w| &w[1])
            .collect();
        assert!(!cited.is_empty(), "claim {n} cites something");
        let mut table_schema: Option<TableSchema> = None;
        for c in &cited {
            let handle = c.split('#').next().unwrap().split('@').next().unwrap();
            let sp = by_handle.get(handle).copied();
            let citation =
                Citation::parse_cli(c, sp).unwrap_or_else(|e| panic!("claim {n}: {c}: {e}"));
            if let Citation::Cells { .. } = citation {
                table_schema = Some(TableSchema::from_spec(
                    sp.expect("a spec for a cells citation"),
                ));
            }
        }
        // Steps check against the cited specs and give the final schema.
        let steps_path = dir.join("steps.json");
        if steps_path.exists() {
            let steps = Steps::from_json(&read_json(&steps_path))
                .unwrap_or_else(|e| panic!("claim {n}: {e}"));
            let schemas: BTreeMap<String, TableSchema> = steps
                .tables
                .iter()
                .map(|(name, h)| {
                    let sp = by_handle[format!("F{}", h.file.0).as_str()];
                    (name.clone(), TableSchema::from_spec(sp))
                })
                .collect();
            table_schema = Some(
                steps
                    .check(&schemas)
                    .unwrap_or_else(|e| panic!("claim {n}: {e}")),
            );
        }
        let assert_path = dir.join("assert.json");
        if assert_path.exists() {
            let a = Assertion::from_json(&read_json(&assert_path))
                .unwrap_or_else(|e| panic!("claim {n}: {e}"));
            let schema = table_schema
                .as_ref()
                .unwrap_or_else(|| panic!("claim {n}: an assertion needs a final table"));
            a.check(schema).unwrap_or_else(|e| panic!("claim {n}: {e}"));
        } else {
            assert!(
                !steps_path.exists(),
                "claim {n}: steps without an assertion"
            );
        }
    }
    // Ten spans, C1 to C10, in the report.
    let report = fs::read_to_string(root.join("report.md")).unwrap();
    for n in 1..=10 {
        assert!(
            report.contains(&format!("{{.claim ref=C{n}}}")),
            "span C{n}"
        );
    }
}

#[test]
fn registration_on_fixtures() {
    let files = [
        "de.tsv",
        "samples.tsv",
        "counts.tsv",
        "figure.pdf",
        "figure.png",
    ];
    for (i, name) in files.iter().enumerate() {
        let (id, ev) =
            Identity::register(FileHandle(i as u64 + 1), name, &registration(name)).unwrap();
        assert_eq!(id.level, Level::Registered);
        assert_eq!(ev.ty(), EventType::FileRegistered);
        assert_eq!(ev.get("path"), Some(&json!(name)));
        assert_eq!(ev.get("kind"), Some(&json!("input")));
        let again = Identity::from_registered(&Value::Object(ev.body().clone())).unwrap();
        assert_eq!(again.path, id.path);
    }
    let code = |r: Result<(Identity, KnownEvent), Error>| r.unwrap_err().catalog_code();
    assert_eq!(
        code(Identity::register(
            FileHandle(9),
            "x.tsv",
            &Registration {
                title: " ".into(),
                ..registration("t")
            }
        )),
        Some(Code::E3002)
    );
    assert_eq!(
        code(Identity::register(
            FileHandle(9),
            "x.tsv",
            &Registration {
                description: String::new(),
                ..registration("t")
            }
        )),
        Some(Code::E3002)
    );
    for bad in ["/abs/x.tsv", "", "a//b", "./x", "a/../b", "dir\\x"] {
        assert_eq!(
            check_input_path(bad).unwrap_err().catalog_code(),
            Some(Code::E3001),
            "{bad:?}"
        );
    }
    assert_eq!(
        check_input_path("data/caf\u{65}\u{301}.tsv").unwrap(),
        "data/caf\u{e9}.tsv",
        "NFC"
    );
    // Rename, removable, restore produce their events through the 6.3 machine.
    let (id, _) = Identity::register(FileHandle(1), "de.tsv", &registration("de")).unwrap();
    let (renamed, ev) = id.rename("tables/de.tsv").unwrap();
    assert_eq!(renamed.path, "tables/de.tsv");
    assert_eq!(ev.get("from"), Some(&json!("de.tsv")));
    let (removable, ev) = renamed.mark_removable().unwrap();
    assert!(removable.removable);
    assert_eq!(ev.ty(), EventType::FileMarkedRemovable);
    assert_eq!(
        removable.mark_removable().unwrap_err().catalog_code(),
        Some(Code::E3201)
    );
    assert_eq!(
        removable.restore().unwrap_err().catalog_code(),
        Some(Code::E3201)
    );
    let retired = Identity {
        retired: true,
        ..removable.clone()
    };
    assert_eq!(
        retired.rename("z").unwrap_err().catalog_code(),
        Some(Code::E3201)
    );
    let (restored, ev) = retired.restore().unwrap();
    assert!(!restored.retired);
    assert_eq!(ev.ty(), EventType::FileRestored);
}

#[test]
fn fixture_tables_parse_with_item_47_types_and_unique_keys() {
    let tmp = tempfile::tempdir().unwrap();
    let st = settings(tmp.path());
    let root = fixtures();
    let de = parse(&root.join("de.tsv"), &spec("de"), &st).unwrap();
    assert_eq!(de.num_rows(), 12);
    let types: Vec<DataType> = de
        .schema()
        .fields()
        .iter()
        .map(|f| f.data_type().clone())
        .collect();
    assert_eq!(
        types,
        [
            DataType::Utf8,
            DataType::Decimal128(38, 2),
            DataType::Decimal128(38, 4),
            DataType::Decimal128(38, 1)
        ]
    );
    let rows = typed_rows(&de, &spec("de")).unwrap();
    assert_eq!(rows[9][0], s("G10"));
    assert_eq!(rows[9][1], dec("-2.40"));
    assert_eq!(rows[11][2], TypedValue::Missing, "NA reads as missing");
    assert_eq!(rows[0][3], dec("150.5"));
    let samples = parse(&root.join("samples.tsv"), &spec("samples"), &st).unwrap();
    assert_eq!(samples.num_rows(), 6);
    let keys = key_tuples(&samples, &spec("samples")).unwrap();
    assert_eq!(
        keys[4],
        [
            s("treated"),
            TypedValue::from_text("2", tm_formats::typed_value::SpecType::Int).unwrap()
        ]
    );
    let counts = parse(&root.join("counts.tsv"), &spec("counts"), &st).unwrap();
    assert_eq!(counts.num_rows(), 12);
    assert_eq!(
        counts.schema().field(1).data_type(),
        &DataType::Decimal128(38, 0)
    );

    // table.spec: a draft without scales gets the largest scale seen, and
    // the completed spec is the committed one.
    let mut draft = read_json(&root.join("specs/de.json"));
    for c in draft["columns"].as_array_mut().unwrap() {
        c.as_object_mut().unwrap().remove("scale");
    }
    let table = complete_spec(&root.join("de.tsv"), &draft, &st).unwrap();
    assert_eq!(table.spec.fingerprint(), spec("de").fingerprint());
    assert_eq!(table.batch.num_rows(), 12);
}

#[test]
fn a_spec_with_each_type_and_the_parse_failures() {
    let tmp = tempfile::tempdir().unwrap();
    let st = settings(tmp.path());
    let path = tmp.path().join("every.csv");
    fs::write(
        &path,
        "name,n,d,f,b,day,at\n\
         \"a,b\",42,1.50,1e-05,true,2026-11-09,2026-11-09T14:02:11Z\n\
         \"q\"\"r\",NA,-0.5,NaN,false,2026-01-01,2026-01-01T00:00:00.5Z\n\
         ,0,7,inf,true,2000-02-29,1999-12-31T23:59:59Z\n",
    )
    .unwrap();
    let mut every = json!({
        "spec": 1, "format": "csv", "delimiter": ",", "header": true,
        "columns": [
            {"name": "name", "type": "str", "missing": []},
            {"name": "n", "type": "int", "missing": ["NA"]},
            {"name": "d", "type": "dec", "missing": []},
            {"name": "f", "type": "float64", "missing": []},
            {"name": "b", "type": "bool", "missing": []},
            {"name": "day", "type": "date", "missing": []},
            {"name": "at", "type": "datetime", "missing": []}
        ],
        "key": ["name"], "nan": "value", "inf": "missing", "encoding": "utf-8"
    });
    let table = complete_spec(&path, &every, &st).unwrap();
    assert_eq!(table.spec.column("d").unwrap().scale, Some(2));
    let types: Vec<DataType> = table
        .batch
        .schema()
        .fields()
        .iter()
        .map(|f| f.data_type().clone())
        .collect();
    assert_eq!(
        types,
        [
            DataType::Utf8,
            DataType::Decimal128(38, 0),
            DataType::Decimal128(38, 2),
            DataType::Float64,
            DataType::Boolean,
            DataType::Date32,
            DataType::Timestamp(TimeUnit::Nanosecond, Some("UTC".into())),
        ]
    );
    let rows = typed_rows(&table.batch, &table.spec).unwrap();
    assert_eq!(rows[0][0], s("a,b"));
    assert_eq!(rows[1][0], s("q\"r"));
    assert_eq!(rows[2][0], s(""), "an empty field is the empty token");
    assert_eq!(rows[1][1], TypedValue::Missing);
    assert_eq!(
        rows[0][3],
        dec("0.00001"),
        "float64 is read as an IEEE double, shortest round trip"
    );
    assert_eq!(rows[1][3], TypedValue::Nan, "nan: value");
    assert_eq!(rows[2][3], TypedValue::Missing, "inf: missing");
    assert_eq!(rows[1][6].to_json()["v"], "2026-01-01T00:00:00.5Z");

    let code = |r: Result<arrow::array::RecordBatch, Error>| r.unwrap_err();
    // A cell that does not read as its type names row and column.
    fs::write(tmp.path().join("bad.csv"), "k,n\na,1\nb,x\n").unwrap();
    let spec_kn = ParseSpec::from_json(&json!({
        "spec": 1, "format": "csv", "delimiter": ",", "header": true,
        "columns": [{"name": "k", "type": "str", "missing": []}, {"name": "n", "type": "int", "missing": []}],
        "key": ["k"], "nan": "missing", "inf": "missing", "encoding": "utf-8"
    }))
    .unwrap();
    let e = code(parse(&tmp.path().join("bad.csv"), &spec_kn, &st));
    assert_eq!(e.catalog_code(), Some(Code::E3102));
    assert!(
        e.to_string().contains("row 2") && e.to_string().contains("\"n\""),
        "{e}"
    );
    // An integer wider than 38 digits.
    fs::write(
        tmp.path().join("wide.csv"),
        "k,n\na,123456789012345678901234567890123456789\n",
    )
    .unwrap();
    assert_eq!(
        code(parse(&tmp.path().join("wide.csv"), &spec_kn, &st)).catalog_code(),
        Some(Code::E3102)
    );
    // nan in a non-float64 column.
    fs::write(tmp.path().join("nan.csv"), "k,n\na,NaN\n").unwrap();
    assert_eq!(
        code(parse(&tmp.path().join("nan.csv"), &spec_kn, &st)).catalog_code(),
        Some(Code::E3102)
    );
    // A spec column the header lacks.
    fs::write(tmp.path().join("short.csv"), "k\na\n").unwrap();
    assert_eq!(
        code(parse(&tmp.path().join("short.csv"), &spec_kn, &st)).catalog_code(),
        Some(Code::E3103)
    );
    // A longer fraction than the spec's scale on a later version.
    fs::write(
        tmp.path().join("scale.csv"),
        "name,n,d,f,b,day,at\nz,1,1.234,1,true,2026-11-09,2026-11-09T14:02:11Z\n",
    )
    .unwrap();
    every["columns"][2]["scale"] = json!(2);
    let fixed = ParseSpec::from_json(&every).unwrap();
    assert_eq!(
        code(parse(&tmp.path().join("scale.csv"), &fixed, &st)).catalog_code(),
        Some(Code::E3102)
    );
    // No header: columns are positional.
    fs::write(tmp.path().join("nohead.tsv"), "a\t1\nb\t2\n").unwrap();
    let nohead = ParseSpec::from_json(&json!({
        "spec": 1, "format": "tsv", "delimiter": "\t", "header": false,
        "columns": [{"name": "k", "type": "str", "missing": []}, {"name": "n", "type": "int", "missing": []}],
        "key": ["k"], "nan": "missing", "inf": "missing", "encoding": "utf-8"
    }))
    .unwrap();
    assert_eq!(
        parse(&tmp.path().join("nohead.tsv"), &nohead, &st)
            .unwrap()
            .num_rows(),
        2
    );
}

#[test]
fn a_duplicate_key_is_e3101_with_the_values() {
    let tmp = tempfile::tempdir().unwrap();
    let st = settings(tmp.path());
    let path = tmp.path().join("dup.tsv");
    fs::write(
        &path,
        "sample_id\tcondition\tbatch\nS1\tcontrol\t1\nS2\tcontrol\t2\nS9\tcontrol\t1\n",
    )
    .unwrap();
    let e = parse(&path, &spec("samples"), &st).unwrap_err();
    assert_eq!(e.catalog_code(), Some(Code::E3101));
    let msg = e.to_string();
    assert!(
        msg.contains("control") && msg.contains("rows 1 and 3"),
        "{msg}"
    );
}

#[test]
fn each_equivalence_rule() {
    let tmp = tempfile::tempdir().unwrap();
    let st = settings(tmp.path());
    let root = fixtures();
    let hs = HashSettings::default();
    let de_path = root.join("de.tsv");
    let de_spec = spec("de");
    let first = Candidate::of_file(&de_path, &hs).unwrap();
    let (v1, ev) = Version::next(FileHandle(1), None, &first, Origin::Registered, false)
        .unwrap()
        .unwrap();
    assert_eq!(ev.get("content_label"), Some(&Value::Null));
    assert_eq!(ev.get("manifest"), Some(&Value::Null));
    assert_eq!(v1.handle, VersionHandle::parse("F1@v1").unwrap());
    let table1 = parse(&de_path, &de_spec, &st).unwrap();

    // Same bytes: a copy.
    let copy = tmp.path().join("copy.tsv");
    fs::copy(&de_path, &copy).unwrap();
    let c = Candidate::of_file(&copy, &hs).unwrap();
    assert_eq!(
        equivalence(&v1, &c, None, &copy).unwrap(),
        Equivalence::SameBytes
    );
    assert!(
        Version::next(
            FileHandle(1),
            Some(&v1),
            &c,
            Origin::ChangedOutsideRun,
            true
        )
        .unwrap()
        .is_none()
    );

    // Same table by key: rows shuffled, one value respelled (1.9 for 1.90).
    let text = fs::read_to_string(&de_path).unwrap();
    let mut lines: Vec<&str> = text.lines().collect();
    let header = lines.remove(0);
    lines.reverse();
    let shuffled = tmp.path().join("shuffled.tsv");
    let body: String = lines
        .iter()
        .map(|l| l.replace("\t1.90\t", "\t1.9\t") + "\n")
        .collect();
    fs::write(&shuffled, format!("{header}\n{body}")).unwrap();
    let c = Candidate::of_file(&shuffled, &hs).unwrap();
    let table2 = parse(&shuffled, &de_spec, &st).unwrap();
    assert!(same_table_by_key(&table1, &table2, &de_spec).unwrap());
    assert_eq!(
        equivalence(&v1, &c, Some((&table1, &table2, &de_spec)), &shuffled).unwrap(),
        Equivalence::SameTableByKey
    );

    // Different: one value changed; a new version, labeled content_changed.
    let edited = tmp.path().join("edited.tsv");
    fs::write(&edited, text.replace("\t-2.40\t", "\t-2.50\t")).unwrap();
    let c = Candidate::of_file(&edited, &hs).unwrap();
    let table3 = parse(&edited, &de_spec, &st).unwrap();
    assert!(!same_table_by_key(&table1, &table3, &de_spec).unwrap());
    assert_eq!(
        equivalence(&v1, &c, Some((&table1, &table3, &de_spec)), &edited).unwrap(),
        Equivalence::Different
    );
    let (v2, ev) = Version::next(
        FileHandle(1),
        Some(&v1),
        &c,
        Origin::ChangedOutsideRun,
        false,
    )
    .unwrap()
    .unwrap();
    assert_eq!(v2.handle.version, 2);
    assert_eq!(v2.content_label, Some(ContentLabel::ContentChanged));
    assert_eq!(ev.get("origin").unwrap()["kind"], "changed_outside_run");
    let back = Version::from_added(&Value::Object(ev.body().clone())).unwrap();
    assert_eq!(back.fingerprint, v2.fingerprint);

    // Decompressed content: not built (rule 4 collision, see the P3 report).
    let gz = tmp.path().join("de.tsv.gz");
    fs::write(&gz, b"\x1f\x8b\x08\0\0\0\0\0\0\x03").unwrap();
    assert!(matches!(
        decompressed_fingerprint(&gz),
        Err(Error::Unbuilt { .. })
    ));
    let c = Candidate::of_file(&gz, &hs).unwrap();
    assert!(matches!(
        equivalence(&v1, &c, None, &gz),
        Err(Error::Unbuilt { .. })
    ));

    // Keeping goes through 6.3.
    let state = State {
        level: Level::Registered,
        is_input: true,
        retired: false,
        removable: false,
        latest: Some(Kept::No),
    };
    let object = v2.fingerprint.clone();
    let (kept, ev) = v2
        .kept(state, &object, 100, KeepReason::Owner, false)
        .unwrap();
    assert_eq!(kept.kept, Kept::Bytes);
    assert_eq!(ev.get("reason"), Some(&json!("owner")));
    let state_kept = State {
        latest: Some(Kept::Bytes),
        ..state
    };
    assert_eq!(
        kept.kept(state_kept, &object, 100, KeepReason::Promotion, false)
            .unwrap_err()
            .catalog_code(),
        Some(Code::E3201)
    );
    let tracked = State {
        level: Level::Tracked,
        ..state
    };
    assert_eq!(
        v2.kept(tracked, &object, 100, KeepReason::Promotion, false)
            .unwrap_err()
            .catalog_code(),
        Some(Code::E3201)
    );
    let _ = tracked.apply(Action::Register).unwrap();
}

/// A directory version's `file.version_added` body carries the manifest
/// whose fingerprint `tm-fsfacts` computes under the P2 ruling
/// (`[{path, fingerprint}]`), while `tm-formats` still checks the P1 rule
/// (the ID of `entries`). The two cannot both hold; the P3 report carries
/// this as a rule 4 collision, and this test pins the state until it is
/// ruled.
#[test]
fn directory_version_event_collides_with_the_p1_manifest_check() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("raw");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("a.fastq"), b"@r1\nACGT\n+\nIIII\n").unwrap();
    let c = Candidate::of_directory(&dir, &[], &HashSettings::default()).unwrap();
    assert!(c.manifest.is_some());
    let err = Version::next(FileHandle(9), None, &c, Origin::Registered, false).unwrap_err();
    let Error::Format(e) = err else {
        panic!("expected the body validation to refuse: {err}");
    };
    assert_eq!(e.code, Code::E1002);
    assert_eq!(
        e.field.as_deref(),
        Some("/body/manifest/fingerprint"),
        "{e}"
    );
}

#[test]
fn parse_cache_round_trips_and_parquet_parses() {
    let tmp = tempfile::tempdir().unwrap();
    let st = settings(tmp.path());
    let root = fixtures();
    let de_spec = spec("de");
    let table = parse(&root.join("de.tsv"), &de_spec, &st).unwrap();
    let cache = ParseCache::new(tmp.path().join("cache"));
    let file_fp = Candidate::of_file(&root.join("de.tsv"), &HashSettings::default())
        .unwrap()
        .fingerprint;
    assert!(
        cache
            .load(&file_fp, &de_spec.fingerprint())
            .unwrap()
            .is_none()
    );
    let path = cache
        .store(&file_fp, &de_spec.fingerprint(), &table)
        .unwrap();
    assert!(path.ends_with(format!("{}.parquet", &de_spec.fingerprint().as_str()[7..])));
    let back = cache
        .load(&file_fp, &de_spec.fingerprint())
        .unwrap()
        .unwrap();
    assert_eq!(
        back.schema(),
        table.schema(),
        "the stored schema restores item 47's types"
    );
    assert!(same_table_by_key(&table, &back, &de_spec).unwrap());
    // The cached file is itself a Parquet table a parquet spec reads.
    let mut pq = read_json(&root.join("specs/de.json"));
    pq["format"] = json!("parquet");
    pq["delimiter"] = Value::Null;
    pq["header"] = Value::Null;
    let pq_spec = ParseSpec::from_json(&pq).unwrap();
    let again = parse(&path, &pq_spec, &st).unwrap();
    assert_eq!(
        typed_rows(&again, &pq_spec).unwrap(),
        typed_rows(&table, &de_spec).unwrap()
    );
}

#[test]
fn pdf_pages_image_panels_and_the_e3301_refusal() {
    let root = fixtures();
    assert_eq!(
        tm_files::pdf::page_count(&root.join("figure.pdf")).unwrap(),
        2
    );
    assert!(matches!(
        tm_files::pdf::page_count(&root.join("de.tsv")),
        Err(Error::Pdf { .. })
    ));
    assert!(tm_files::image::is_image(&root.join("figure.png")).unwrap());
    assert!(!tm_files::image::is_image(&root.join("de.tsv")).unwrap());
    assert!(!tm_files::image::is_image(&root.join("figure.pdf")).unwrap());
    let state = State {
        level: Level::Registered,
        is_input: true,
        retired: false,
        removable: false,
        latest: Some(Kept::No),
    };
    let v = VersionHandle::parse("F5@v1").unwrap();
    let ev = tm_files::image::set_panels(state, &v, true, &["A".into(), "B".into()]).unwrap();
    assert_eq!(ev.get("panels"), Some(&json!(["A", "B"])));
    assert_eq!(
        tm_files::image::set_panels(state, &v, false, &["A".into()])
            .unwrap_err()
            .catalog_code(),
        Some(Code::E3301)
    );
    assert_eq!(
        tm_files::image::set_panels(
            State {
                latest: None,
                ..state
            },
            &v,
            true,
            &["A".into()]
        )
        .unwrap_err()
        .catalog_code(),
        Some(Code::E3201)
    );
}

#[test]
fn both_checksum_forms_on_the_fixture_tables() {
    let root = fixtures();
    let hs = HashSettings::default();
    let entries: Vec<(String, EventId)> = ["de.tsv", "samples.tsv", "counts.tsv"]
        .iter()
        .map(|n| {
            (
                (*n).to_owned(),
                Candidate::of_file(&root.join(n), &hs).unwrap().fingerprint,
            )
        })
        .collect();
    let hex = |fp: &EventId| fp.as_str()[7..].to_owned();
    let mut gnu = String::new();
    for (n, fp) in &entries {
        let _ = writeln!(gnu, "{}  {n}", hex(fp));
    }
    let lines = parse_checksums(&gnu).unwrap();
    assert_eq!(lines.len(), 3);
    assert!(lines.iter().all(|l| l.algorithm == Algorithm::Sha256));
    let c = compare(&lines, &entries).unwrap();
    assert!(c.matched, "{c:?}");
    let ev = c
        .event(FileHandle(1), &VersionHandle::parse("F1@v1").unwrap())
        .unwrap();
    assert_eq!(ev.get("algorithm"), Some(&json!("sha256")));
    let mut bsd = String::new();
    for (n, fp) in &entries {
        let _ = writeln!(bsd, "SHA256 ({n}) = {}", hex(fp));
    }
    let c2 = compare(&parse_checksums(&bsd).unwrap(), &entries).unwrap();
    assert!(c2.matched);
    // One wrong digest and one unknown name are the mismatches.
    let mut wrong = gnu.replace(&hex(&entries[0].1), &"0".repeat(64));
    let _ = writeln!(wrong, "{}  missing.tsv", "1".repeat(64));
    let c3 = compare(&parse_checksums(&wrong).unwrap(), &entries).unwrap();
    assert_eq!(c3.mismatches, ["de.tsv", "missing.tsv"]);
    assert_eq!(c3.refusal().unwrap().catalog_code(), Some(Code::E3004));
    // MD5 parses and cannot be compared (rule 4 collision, see the P3 report).
    let md5 = parse_checksums("d41d8cd98f00b204e9800998ecf8427e  de.tsv\n").unwrap();
    assert_eq!(md5[0].algorithm, Algorithm::Md5);
    assert!(matches!(
        compare(&md5, &entries),
        Err(Error::Unbuilt { .. })
    ));
}

// ------------------------------------------------------------------ fold

const SCHEMA_11_17: &str = "
CREATE TABLE events (id TEXT PRIMARY KEY, chain TEXT NOT NULL, seq INTEGER NOT NULL, type TEXT NOT NULL,
  segment INTEGER NOT NULL, offset INTEGER NOT NULL, at TEXT NOT NULL);
CREATE TABLE files (file TEXT PRIMARY KEY, kind TEXT NOT NULL, path TEXT NOT NULL, title TEXT, description TEXT,
  source TEXT, level TEXT NOT NULL, retired INTEGER NOT NULL DEFAULT 0, removable INTEGER NOT NULL DEFAULT 0,
  agent_denied INTEGER NOT NULL DEFAULT 0, registered_event TEXT);
CREATE INDEX files_path ON files(path);
CREATE TABLE file_versions (version TEXT PRIMARY KEY, file TEXT NOT NULL, n INTEGER NOT NULL, fingerprint TEXT NOT NULL,
  origin_kind TEXT NOT NULL, origin_run TEXT, content_label TEXT, size INTEGER, kept_object TEXT,
  fingerprint_only INTEGER NOT NULL DEFAULT 0, event_id TEXT NOT NULL);
CREATE INDEX file_versions_file ON file_versions(file, n);
CREATE TABLE parse_specs (file TEXT PRIMARY KEY, spec TEXT NOT NULL, spec_fingerprint TEXT NOT NULL, event_id TEXT NOT NULL);
CREATE TABLE panels (version TEXT NOT NULL, name TEXT NOT NULL, PRIMARY KEY (version, name));
";

fn line(seq: u64, prev: &EventId, ev: &KnownEvent) -> Line {
    let head = envelope::Head {
        chain: "main".into(),
        seq,
        prev: prev.clone(),
        at: "2026-11-12T12:00:00Z".into(),
        actor: Actor {
            kind: ActorKind::Owner,
            id: "tim".into(),
        },
        src: Some(Source {
            source: SourceKind::Cli,
            tty: true,
            harness_env: false,
            confirmed_by: None,
        }),
        batch: Some(1),
        epoch: Some(1),
    };
    let (text, _) = envelope::write_line(&head, ev).unwrap();
    envelope::read_line(&text).unwrap()
}

#[test]
fn the_fold_writes_the_four_tables_and_is_idempotent() {
    let root = fixtures();
    let hs = HashSettings::default();
    // The events of a small history over the fixture files.
    let (de, reg) = Identity::register(FileHandle(1), "de.tsv", &registration("DE table")).unwrap();
    let cand = Candidate::of_file(&root.join("de.tsv"), &hs).unwrap();
    let (v1, added) = Version::next(FileHandle(1), None, &cand, Origin::Registered, false)
        .unwrap()
        .unwrap();
    let de_spec = spec("de");
    let spec_set = KnownEvent::new(
        EventType::FileParseSpecSet,
        &json!({"file": "F1", "spec": de_spec.to_json(), "spec_fingerprint": de_spec.fingerprint().as_str()}),
    )
    .unwrap();
    let state = State {
        level: Level::Registered,
        is_input: true,
        retired: false,
        removable: false,
        latest: Some(Kept::No),
    };
    let (_, kept) = v1
        .kept(state, &v1.fingerprint, 321, KeepReason::Owner, true)
        .unwrap();
    let (_, renamed) = de.rename("tables/de.tsv").unwrap();
    let (_, removable) = de.mark_removable().unwrap();
    let (_, restored) = Identity {
        retired: true,
        ..de.clone()
    }
    .restore()
    .unwrap();
    let (_, png) =
        Identity::register(FileHandle(5), "figure.png", &registration("Figure")).unwrap();
    let pcand = Candidate::of_file(&root.join("figure.png"), &hs).unwrap();
    let (_, padded) = Version::next(FileHandle(5), None, &pcand, Origin::Registered, false)
        .unwrap()
        .unwrap();
    let panels = tm_files::image::set_panels(
        state,
        &VersionHandle::parse("F5@v1").unwrap(),
        true,
        &["A".into(), "B".into()],
    )
    .unwrap();
    let panels2 = tm_files::image::set_panels(
        state,
        &VersionHandle::parse("F5@v1").unwrap(),
        true,
        &["B".into(), "C".into()],
    )
    .unwrap();
    let events = [
        reg, added, spec_set, kept, renamed, removable, restored, png, padded, panels, panels2,
    ];
    let mut prev = genesis("main", "p-fixture");
    let mut lines = Vec::new();
    for (i, ev) in events.iter().enumerate() {
        let l = line(i as u64 + 1, &prev, ev);
        prev = l.id.clone();
        lines.push(l);
    }

    let mut index = Index::in_memory().unwrap();
    index.execute_batch(SCHEMA_11_17).unwrap();
    let mut tx = index.begin().unwrap();
    for l in &lines {
        tm_files::fold::fold(&mut tx, l).unwrap();
        tx.execute(
            "INSERT INTO events (id, chain, seq, type, segment, offset, at) VALUES (?, 'main', ?, ?, 1, 0, '2026-11-12T12:00:00Z')",
            &[
                Param::from(l.id.as_str()),
                Param::from(l.envelope["seq"].as_u64().unwrap()),
                Param::from(l.envelope["type"].as_str().unwrap()),
            ],
        )
        .unwrap();
    }
    let files = tx
        .query_rows(
            "SELECT file, path, title, level, removable, retired FROM files ORDER BY file",
            &[],
        )
        .unwrap();
    assert_eq!(files.len(), 2);
    assert_eq!(files[0].text(0), Some("F1"));
    assert_eq!(files[0].text(1), Some("tables/de.tsv"), "renamed");
    assert_eq!(files[0].text(2), Some("DE table"));
    assert_eq!(files[0].text(3), Some("registered"));
    assert_eq!(files[0].int(4), Some(1), "removable");
    assert_eq!(files[0].int(5), Some(0), "restored");
    let versions = tx
        .query_rows("SELECT version, n, fingerprint, origin_kind, content_label, kept_object, fingerprint_only, size FROM file_versions ORDER BY version", &[])
        .unwrap();
    assert_eq!(versions.len(), 2);
    assert_eq!(versions[0].text(0), Some("F1@v1"));
    assert_eq!(versions[0].int(1), Some(1));
    assert_eq!(versions[0].text(2), Some(v1.fingerprint.as_str()));
    assert_eq!(versions[0].text(3), Some("registered"));
    assert!(
        versions[0].get(4).is_null(),
        "a first version has no content label"
    );
    assert_eq!(versions[0].text(5), Some(v1.fingerprint.as_str()), "kept");
    assert_eq!(versions[0].int(6), Some(1), "fingerprint only");
    assert_eq!(versions[0].int(7), Some(i64::try_from(cand.size).unwrap()));
    let specs = tx
        .query_rows("SELECT file, spec_fingerprint FROM parse_specs", &[])
        .unwrap();
    assert_eq!(specs.len(), 1);
    assert_eq!(specs[0].text(1), Some(de_spec.fingerprint().as_str()));
    let panels = tx
        .query_rows(
            "SELECT name FROM panels WHERE version = 'F5@v1' ORDER BY name",
            &[],
        )
        .unwrap();
    let names: Vec<&str> = panels.iter().filter_map(|r| r.text(0)).collect();
    assert_eq!(names, ["B", "C"], "panels_set replaces the list");
    // Folding the same lines again changes nothing: events already holds them.
    for l in &lines {
        tm_files::fold::fold(&mut tx, l).unwrap();
    }
    let again = tx
        .query_rows("SELECT COUNT(*) FROM file_versions", &[])
        .unwrap();
    assert_eq!(again[0].int(0), Some(2));
    tx.commit().unwrap();
}
