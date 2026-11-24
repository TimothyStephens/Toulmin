//! Item 71's claims tests: every cell of the 7.3 table reachable in gate A
//! (and the Retired column, which gate C reaches), the column reading,
//! the combination into `evidence`, and the Claims fold over the 11.17
//! tables (rows, state, flags, `claim_keys`, idempotence, and that a
//! review never changes a verdict).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_lines,
    reason = "a test harness: panics report setup failures"
)]

use std::path::PathBuf;

use serde_json::{Map, Value, json};
use tm_claims::claim::{self, Claim, Content, Kind};
use tm_claims::evidence_exists::{
    Base, CitationType, Evidence, Outcome, Situation, VersionFacts, cell, evaluate, subject,
};
use tm_claims::fold::fold;
use tm_claims::refs::{allocate, highest_owner_ref};
use tm_claims::reviews::{Review, ReviewOutcome};
use tm_formats::citation::Citation;
use tm_formats::envelope::{self, Actor, ActorKind, Line, Source, SourceKind};
use tm_formats::events::{EventType, KnownEvent};
use tm_formats::id::{ClaimRef, EventId, ReportHandle, VersionHandle, genesis};
use tm_index::{Index, IndexTx, Param};

// ------------------------------------------------------------------ 7.3

use CitationType::{Cells, Page, Panel, Reserved, WholeFile};
use Situation::{
    DiskDiffers, FingerprintOnlyMissing, KeptOrMatches, MissingNotKept, RemovableDeletedNotKept,
};

const COLUMNS: [Situation; 6] = [
    KeptOrMatches,
    DiskDiffers { kept: true },
    DiskDiffers { kept: false },
    MissingNotKept,
    RemovableDeletedNotKept,
    FingerprintOnlyMissing,
];

/// 7.3 as written: per row, per column of [`COLUMNS`], the outcome when
/// the pinned test passes, when it fails, `evidence moved`, and
/// `expected-missing`.
#[allow(clippy::type_complexity)]
fn table() -> Vec<(CitationType, [(&'static str, &'static str, bool, bool); 6])> {
    const H: &str = "holds";
    const F: &str = "fails";
    const N: &str = "not_checkable";
    vec![
        (
            Cells,
            [
                (H, F, false, false),
                (H, F, true, false),
                (N, N, true, false),
                (N, N, false, false),
                (N, N, false, true),
                (N, N, false, false),
            ],
        ),
        (
            WholeFile,
            [
                (H, H, false, false),
                (H, H, true, false),
                (H, H, true, false),
                (N, N, false, false),
                (N, N, false, true),
                (N, N, false, false),
            ],
        ),
        (
            Page,
            [
                (H, F, false, false),
                (H, F, true, false),
                (N, N, true, false),
                (N, N, false, false),
                (N, N, false, false),
                (N, N, false, false),
            ],
        ),
        (
            Panel,
            [
                (H, F, false, false),
                (H, H, true, false),
                (H, H, true, false),
                (N, N, false, false),
                (N, N, false, true),
                (N, N, false, false),
            ],
        ),
        (
            Reserved,
            [
                (N, N, false, false),
                (N, N, false, false),
                (N, N, false, false),
                (N, N, false, false),
                (N, N, false, false),
                (N, N, false, false),
            ],
        ),
    ]
}

#[test]
fn every_cell_of_the_outcome_table() {
    for (t, row) in table() {
        for (s, (pass, fail, moved, expected_missing)) in COLUMNS.into_iter().zip(row) {
            for retired in [false, true] {
                let r = evaluate(t, s, retired, || true);
                assert_eq!(r.outcome.as_str(), pass, "{t:?} {s:?} test passes");
                let r = evaluate(t, s, retired, || false);
                assert_eq!(r.outcome.as_str(), fail, "{t:?} {s:?} test fails");
                let rule = cell(t, s, retired);
                assert_eq!(rule.evidence_moved, moved, "{t:?} {s:?} evidence moved");
                assert_eq!(
                    rule.expected_missing, expected_missing,
                    "{t:?} {s:?} expected-missing"
                );
                // Retired: as the matching column, plus `cited file
                // retired`; a reserved citation is `not checkable` and
                // carries nothing.
                assert_eq!(
                    rule.cited_file_retired,
                    retired && t != Reserved,
                    "{t:?} {s:?} retired"
                );
            }
        }
    }
}

#[test]
fn the_pinned_test_runs_only_where_a_cell_needs_it() {
    for (t, _) in table() {
        for s in COLUMNS {
            let mut called = false;
            let _ = evaluate(t, s, false, || {
                called = true;
                true
            });
            assert_eq!(called, cell(t, s, false).base == Base::Check, "{t:?} {s:?}");
        }
    }
}

#[test]
fn the_column_reading() {
    let f = |on_disk, matches, bytes_kept, fingerprint_only, removable| {
        Situation::of(VersionFacts {
            on_disk,
            matches,
            bytes_kept,
            fingerprint_only,
            removable,
        })
    };
    assert_eq!(f(true, true, false, false, false), KeptOrMatches);
    assert_eq!(f(true, true, true, false, false), KeptOrMatches);
    // Disk differs wins over kept.
    assert_eq!(
        f(true, false, true, false, false),
        DiskDiffers { kept: true }
    );
    assert_eq!(
        f(true, false, false, true, false),
        DiskDiffers { kept: false }
    );
    // A kept version missing from disk is in the first column.
    assert_eq!(f(false, false, true, false, true), KeptOrMatches);
    assert_eq!(f(false, false, false, false, false), MissingNotKept);
    assert_eq!(f(false, false, false, true, false), FingerprintOnlyMissing);
    // Removable wins over fingerprint only.
    assert_eq!(f(false, false, false, true, true), RemovableDeletedNotKept);
    assert_eq!(f(false, false, false, false, true), RemovableDeletedNotKept);
}

#[test]
fn citations_combine_into_one_evidence() {
    let holds = evaluate(WholeFile, KeptOrMatches, false, || true);
    let fails = evaluate(Cells, KeptOrMatches, false, || false);
    let nc = evaluate(Page, MissingNotKept, false, || true);
    let reserved = evaluate(Reserved, KeptOrMatches, false, || true);

    let e = Evidence::combine(&[("F1@v1".into(), holds.clone())]);
    assert_eq!(e.outcome, Outcome::Holds);
    assert_eq!(e.to_json(), json!({"outcome": "holds", "detail": null}));

    let e = Evidence::combine(&[
        ("F1@v1".into(), holds.clone()),
        ("F4@v1".into(), nc.clone()),
        ("F2@v1".into(), fails),
        ("quote".into(), reserved),
    ]);
    assert_eq!(e.outcome, Outcome::Fails);
    assert_eq!(
        e.detail.as_deref(),
        Some(
            "1 F4@v1: missing, not kept; 2 F2@v1: a cited key is absent; \
             3 quote: reserved citation kind"
        )
    );

    let e = Evidence::combine(&[("F1@v1".into(), holds), ("F4@v1".into(), nc)]);
    assert_eq!(e.outcome, Outcome::NotCheckable);
    assert_eq!(e.detail.as_deref(), Some("1 F4@v1: missing, not kept"));

    let e = Evidence::combine(&[]);
    assert_eq!(
        e.to_json(),
        json!({"outcome": "not_checkable", "detail": null})
    );

    let reasons: Vec<&str> = [
        evaluate(Page, KeptOrMatches, false, || false),
        evaluate(Panel, KeptOrMatches, false, || false),
        evaluate(Cells, DiskDiffers { kept: false }, false, || true),
        evaluate(Cells, RemovableDeletedNotKept, false, || true),
        evaluate(Cells, FingerprintOnlyMissing, false, || true),
    ]
    .iter()
    .map(|r| r.reason.unwrap())
    .collect();
    assert_eq!(
        reasons,
        [
            "page beyond the page count",
            "panel not registered",
            "disk differs, bytes not kept",
            "removable and deleted, not kept",
            "fingerprint only and missing"
        ]
    );

    let quote = Citation::from_json(&json!({"kind": "quote", "text": "x"}), "").unwrap();
    assert_eq!(subject(&quote), "quote");
    assert_eq!(CitationType::of(&quote), Reserved);
}

// ------------------------------------------------------------------ fold

/// 11.17's `schema.sql`, read from the design so the test cannot drift
/// from it.
fn schema() -> String {
    let design = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/design/toulmin_v1_design.md"),
    )
    .unwrap();
    let start = design.find("#### 11.17 Index schema").unwrap();
    let rest = &design[start..];
    let sql = &rest[rest.find("```sql\n").unwrap() + 7..];
    sql[..sql.find("```").unwrap()].to_owned()
}

fn index() -> Index {
    let index = Index::in_memory().unwrap();
    index.execute_batch(&schema()).unwrap();
    index
}

struct Chain {
    seq: u64,
    prev: EventId,
}

impl Chain {
    fn new() -> Chain {
        Chain {
            seq: 0,
            prev: genesis("main", "p1"),
        }
    }

    fn line(&mut self, ev: &KnownEvent) -> Line {
        self.seq += 1;
        let head = envelope::Head {
            chain: "main".into(),
            seq: self.seq,
            prev: self.prev.clone(),
            at: format!("2026-11-24T12:00:{:02}Z", self.seq),
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
            batch: Some(self.seq),
            epoch: Some(1),
        };
        let (text, id) = envelope::write_line(&head, ev).unwrap();
        self.prev = id;
        envelope::read_line(&text).unwrap()
    }
}

/// Fold a line and record it in `events` as the Core would.
fn apply(tx: &mut IndexTx<'_>, line: &Line) {
    fold(tx, line).unwrap();
    tx.execute(
        "INSERT INTO events (id, chain, seq, type, segment, offset, at) VALUES (?, 'main', ?, ?, 1, 0, ?)",
        &[
            line.id.as_str().into(),
            line.envelope["seq"].as_u64().into(),
            line.envelope["type"].as_str().into(),
            line.envelope["at"].as_str().into(),
        ],
    )
    .unwrap();
}

fn rows(tx: &mut IndexTx<'_>, sql: &str) -> Vec<Vec<Param>> {
    tx.query_rows(sql, &[])
        .unwrap()
        .into_iter()
        .map(|r| r.0)
        .collect()
}

fn t(s: &str) -> Param {
    Param::Text(s.into())
}

fn v(s: &str) -> VersionHandle {
    VersionHandle::parse(s).unwrap()
}

fn c2(sentence: &str) -> Claim {
    Claim {
        claim_ref: ClaimRef::Owner(2),
        kind: Kind::Quantitative,
        report: ReportHandle(1),
        supersedes: None,
        content: Content {
            sentence: sentence.into(),
            cited: vec![v("F1@v1")],
            citations: vec![Citation::WholeFile {
                version: v("F1@v1"),
            }],
            steps: Some(json!({"steps": 1, "tables": {"de": "F1@v1"}, "from": "de",
                "ops": [{"op": "filter", "v": 1, "where": "padj < 0.01"}]})),
            assertion: Some(json!({"assert": 1, "kind": "value",
                "row": [{"t": "str", "v": "G10"}], "column": "log2FoldChange", "op": "eq",
                "expected": {"t": "dec", "v": "-2.50"}, "tol": {"abs": "0.01", "rel": "0"}})),
            qualifiers: None,
        },
    }
}

/// The Checker's part of a `claim.checked` body.
fn checker_part(outcome: &str, keys: Option<&[&str]>) -> Map<String, Value> {
    let error = outcome == "error";
    let body = json!({
        "assertion": if error { Value::Null } else { json!({"outcome": outcome,
            "observed": {"cell": {"t": "dec", "v": "-2.40"}},
            "expected": {"t": "dec", "v": "-2.50"}}) },
        "steps": if error { json!([]) } else { json!([{"op": "filter", "rows_out": 3,
            "description": "Keep rows where padj < 0.01; 3 of 12 rows remain."}]) },
        "final_rows_hash": if error { Value::Null } else { json!(format!("sha256:{}", "d".repeat(64))) },
        "key_map_hash": if error { Value::Null } else { json!(format!("sha256:{}", "e".repeat(64))) },
        "key_map": keys.map_or(Value::Null, |k| json!(k.iter().map(|g| json!([{"t": "str", "v": g}])).collect::<Vec<_>>())),
        "sql": "SELECT ...",
        "versions": {"parse_specs": {"F1@v1": format!("sha256:{}", "a".repeat(64))},
            "translator": "1", "duckdb": "2.0.0", "extensions": [], "rules": "1", "build": "musl"},
        "error": if error { json!({"code": "E5001", "message": "checker error"}) } else { Value::Null },
    });
    body.as_object().unwrap().clone()
}

fn checked(
    claim_ref: ClaimRef,
    revision: &EventId,
    against: &[String],
    outcome: &str,
    keys: Option<&[&str]>,
) -> KnownEvent {
    let evidence = Evidence::combine(&[(
        "F1@v1".into(),
        evaluate(WholeFile, KeptOrMatches, false, || true),
    )]);
    claim::checked_event(
        claim_ref,
        revision,
        against,
        &evidence,
        &checker_part(outcome, keys),
    )
    .unwrap()
}

fn flags(tx: &mut IndexTx<'_>) -> Vec<Vec<Param>> {
    rows(
        tx,
        "SELECT subject_kind, subject, name, reason FROM flags ORDER BY subject, name",
    )
}

fn report_fp() -> EventId {
    EventId::parse(&format!("sha256:{}", "f".repeat(64))).unwrap()
}

#[test]
fn the_fold_writes_its_tables() {
    let mut index = index();
    let mut tx = index.begin().unwrap();
    let mut chain = Chain::new();

    let report = chain.line(
        &KnownEvent::new(
            EventType::ReportRegistered,
            &json!({"report": "T1", "path": "report.md"}),
        )
        .unwrap(),
    );
    apply(&mut tx, &report);
    assert_eq!(
        rows(&mut tx, "SELECT report, path, event_id FROM reports"),
        vec![vec![t("T1"), t("report.md"), t(report.id.as_str())]]
    );

    // claim.added: claims, claim_revisions, claim_cites, claim_events, and
    // `promoted, not reviewed`.
    let added_ev = c2("G10 is downregulated 2.5-fold").added_event().unwrap();
    assert_eq!(
        Claim::from_body(&added_ev),
        Some(c2("G10 is downregulated 2.5-fold"))
    );
    let added = chain.line(&added_ev);
    apply(&mut tx, &added);
    let a = added.id.as_str();
    assert_eq!(
        rows(
            &mut tx,
            "SELECT ref, id, kind, state, created_event, current_revision, report, supersedes, superseded_by FROM claims"
        ),
        vec![vec![
            t("C2"),
            t(a),
            t("quantitative"),
            t("added"),
            t(a),
            t(a),
            t("T1"),
            Param::Null,
            Param::Null
        ]]
    );
    assert_eq!(
        rows(
            &mut tx,
            "SELECT event_id, ref, sentence, cited, qualifiers, at FROM claim_revisions"
        ),
        vec![vec![
            t(a),
            t("C2"),
            t("G10 is downregulated 2.5-fold"),
            t("[\"F1@v1\"]"),
            Param::Null,
            t("2026-11-24T12:00:02Z")
        ]]
    );
    assert_eq!(
        rows(&mut tx, "SELECT ref, version FROM claim_cites"),
        vec![vec![t("C2"), t("F1@v1")]]
    );
    assert_eq!(
        flags(&mut tx),
        vec![vec![
            t("claim"),
            t("C2"),
            t("promoted, not reviewed"),
            t("No review yet")
        ]]
    );

    // claim.checked, fails: verdicts, claim_keys, `assertion fails`.
    let against = claim::against(Some(v("F1@v1")), &[v("F1@v1")]);
    let chk = chain.line(&checked(
        ClaimRef::Owner(2),
        &added.id,
        &against,
        "fails",
        Some(&["G01", "G10", "G11"]),
    ));
    apply(&mut tx, &chk);
    let verdict = rows(
        &mut tx,
        "SELECT event_id, ref, revision, against, evidence_outcome, assertion_outcome, body FROM verdicts",
    );
    assert_eq!(verdict.len(), 1);
    assert_eq!(
        &verdict[0][..6],
        &[
            t(chk.id.as_str()),
            t("C2"),
            t(a),
            t("[\"F1@v1\"]"),
            t("holds"),
            t("fails")
        ]
    );
    assert_eq!(
        verdict[0][6],
        t(&tm_formats::canonical::canonical(&chk.envelope["body"]).unwrap())
    );
    assert_eq!(
        rows(
            &mut tx,
            "SELECT version, key_tuple FROM claim_keys ORDER BY key_tuple"
        ),
        vec![
            vec![t("F1@v1"), t("[{\"t\":\"str\",\"v\":\"G01\"}]")],
            vec![t("F1@v1"), t("[{\"t\":\"str\",\"v\":\"G10\"}]")],
            vec![t("F1@v1"), t("[{\"t\":\"str\",\"v\":\"G11\"}]")],
        ]
    );
    assert_eq!(
        flags(&mut tx),
        vec![
            vec![
                t("claim"),
                t("C2"),
                t("assertion fails"),
                t("the assertion check failed against F1@v1")
            ],
            vec![
                t("claim"),
                t("C2"),
                t("promoted, not reviewed"),
                t("No review yet")
            ],
        ]
    );

    // claim.reviewed: a review line, state reviewed, `promoted, not
    // reviewed` cleared; the verdict and `assertion fails` are untouched.
    let review = Review {
        claim_ref: ClaimRef::Owner(2),
        revision: added.id.clone(),
        outcome: ReviewOutcome::Disputed,
        note: Some("overstated".into()),
        report_fingerprint: report_fp(),
    };
    assert_eq!(
        review.event(None).unwrap_err().catalog_code(),
        Some(tm_formats::Code::E6201)
    );
    let verdicts_before = rows(&mut tx, "SELECT * FROM verdicts");
    let rev = chain.line(&review.event(Some(tm_claims::State::Added)).unwrap());
    apply(&mut tx, &rev);
    assert_eq!(rows(&mut tx, "SELECT * FROM verdicts"), verdicts_before);
    assert_eq!(
        rows(&mut tx, "SELECT ref, revision, outcome, note FROM reviews"),
        vec![vec![t("C2"), t(a), t("disputed"), t("overstated")]]
    );
    assert_eq!(
        rows(&mut tx, "SELECT state FROM claims"),
        vec![vec![t("reviewed")]]
    );
    assert_eq!(
        flags(&mut tx),
        vec![vec![
            t("claim"),
            t("C2"),
            t("assertion fails"),
            t("the assertion check failed against F1@v1")
        ]]
    );

    // claim.revised: a revision row, current_revision moves, cites
    // replaced, state kept, `assertion fails` cleared.
    let mut content = c2("G10 is downregulated 2.4-fold").content;
    content.cited = vec![v("F1@v1"), v("F2@v1")];
    let revised = chain.line(&claim::revised_event(ClaimRef::Owner(2), &content).unwrap());
    apply(&mut tx, &revised);
    assert_eq!(
        rows(&mut tx, "SELECT state, current_revision FROM claims"),
        vec![vec![t("reviewed"), t(revised.id.as_str())]]
    );
    assert_eq!(
        rows(&mut tx, "SELECT version FROM claim_cites ORDER BY version"),
        vec![vec![t("F1@v1")], vec![t("F2@v1")]]
    );
    assert_eq!(
        rows(&mut tx, "SELECT COUNT(*) FROM claim_revisions"),
        vec![vec![Param::Int(2)]]
    );
    assert_eq!(flags(&mut tx).len(), 0);

    // A holding re-check against the same version replaces the pair's
    // keys; one against a new version leaves the old pair; an error
    // verdict's null key map deletes its pair.
    let chk2 = chain.line(&checked(
        ClaimRef::Owner(2),
        &revised.id,
        &against,
        "holds",
        Some(&["G10"]),
    ));
    apply(&mut tx, &chk2);
    let new_against = claim::against(Some(v("F1@v2")), &[v("F1@v2")]);
    let chk3 = chain.line(&checked(
        ClaimRef::Owner(2),
        &revised.id,
        &new_against,
        "holds",
        Some(&["G10", "G11"]),
    ));
    apply(&mut tx, &chk3);
    assert_eq!(
        rows(
            &mut tx,
            "SELECT version, COUNT(*) FROM claim_keys GROUP BY version ORDER BY version"
        ),
        vec![
            vec![t("F1@v1"), Param::Int(1)],
            vec![t("F1@v2"), Param::Int(2)]
        ]
    );
    let chk4 = chain.line(&checked(
        ClaimRef::Owner(2),
        &revised.id,
        &new_against,
        "error",
        None,
    ));
    apply(&mut tx, &chk4);
    assert_eq!(
        rows(
            &mut tx,
            "SELECT version, COUNT(*) FROM claim_keys GROUP BY version"
        ),
        vec![vec![t("F1@v1"), Param::Int(1)]]
    );
    assert_eq!(
        rows(
            &mut tx,
            &format!(
                "SELECT assertion_outcome FROM verdicts WHERE event_id = '{}'",
                chk4.id.as_str()
            )
        ),
        vec![vec![Param::Null]]
    );
    assert_eq!(flags(&mut tx).len(), 0);

    // Supersede: the replacement's claim.added, a failing check on the old
    // ref clears on supersede, and a later failing check of the superseded
    // ref raises nothing.
    let fail_old = chain.line(&checked(
        ClaimRef::Owner(2),
        &revised.id,
        &against,
        "fails",
        Some(&[]),
    ));
    apply(&mut tx, &fail_old);
    assert_eq!(flags(&mut tx).len(), 1);
    let mut c3 = c2("G10 is downregulated 2.4-fold (log2)");
    c3.claim_ref = ClaimRef::Owner(3);
    c3.supersedes = Some(ClaimRef::Owner(2));
    let added3 = chain.line(&c3.added_event().unwrap());
    apply(&mut tx, &added3);
    let sup = chain.line(&claim::superseded_event(ClaimRef::Owner(2), ClaimRef::Owner(3)).unwrap());
    apply(&mut tx, &sup);
    assert_eq!(
        rows(
            &mut tx,
            "SELECT ref, state, supersedes, superseded_by FROM claims ORDER BY ref"
        ),
        vec![
            vec![t("C2"), t("superseded"), Param::Null, t("C3")],
            vec![t("C3"), t("added"), t("C2"), Param::Null],
        ]
    );
    let late = chain.line(&checked(
        ClaimRef::Owner(2),
        &revised.id,
        &against,
        "fails",
        Some(&[]),
    ));
    apply(&mut tx, &late);
    assert_eq!(
        flags(&mut tx),
        vec![vec![
            t("claim"),
            t("C3"),
            t("promoted, not reviewed"),
            t("No review yet")
        ]]
    );
    // A review of the superseded claim leaves its state.
    let rev2 = chain.line(
        &Review {
            revision: revised.id.clone(),
            ..review.clone()
        }
        .event(Some(tm_claims::State::Superseded))
        .unwrap(),
    );
    apply(&mut tx, &rev2);
    assert_eq!(
        rows(&mut tx, "SELECT state FROM claims WHERE ref = 'C2'"),
        vec![vec![t("superseded")]]
    );

    // Retract C3.
    let ret = chain.line(&claim::retracted_event(ClaimRef::Owner(3), "wrong table").unwrap());
    apply(&mut tx, &ret);
    assert_eq!(
        rows(&mut tx, "SELECT state FROM claims WHERE ref = 'C3'"),
        vec![vec![t("retracted")]]
    );

    // claim_events: one row per claim. event under its body's ref.
    assert_eq!(
        rows(&mut tx, "SELECT ref, type FROM claim_events ORDER BY at"),
        vec![
            vec![t("C2"), t("claim.added")],
            vec![t("C2"), t("claim.checked")],
            vec![t("C2"), t("claim.reviewed")],
            vec![t("C2"), t("claim.revised")],
            vec![t("C2"), t("claim.checked")],
            vec![t("C2"), t("claim.checked")],
            vec![t("C2"), t("claim.checked")],
            vec![t("C2"), t("claim.checked")],
            vec![t("C3"), t("claim.added")],
            vec![t("C2"), t("claim.superseded")],
            vec![t("C2"), t("claim.checked")],
            vec![t("C2"), t("claim.reviewed")],
            vec![t("C3"), t("claim.retracted")],
        ]
    );

    // Allocation reads the claims table by number.
    assert_eq!(
        highest_owner_ref(&mut tx).unwrap(),
        Some(ClaimRef::Owner(3))
    );
    assert_eq!(
        allocate(
            highest_owner_ref(&mut tx).unwrap(),
            Some(ClaimRef::Owner(3))
        ),
        Ok(ClaimRef::Owner(4))
    );

    // The fold writes no spans and no other library's tables.
    for table in [
        "spans",
        "files",
        "file_versions",
        "parse_specs",
        "panels",
        "settings",
        "audits",
        "head",
        "opaque_events",
    ] {
        assert_eq!(
            rows(&mut tx, &format!("SELECT COUNT(*) FROM {table}")),
            vec![vec![Param::Int(0)]],
            "{table}"
        );
    }
    tx.commit().unwrap();
}

#[test]
fn the_fold_is_idempotent_and_ignores_other_types() {
    let mut index = index();
    let mut tx = index.begin().unwrap();
    let mut chain = Chain::new();
    let added = chain.line(&c2("G10 is downregulated").added_event().unwrap());
    apply(&mut tx, &added);
    let snapshot = |tx: &mut IndexTx<'_>| {
        [
            "claims",
            "claim_revisions",
            "claim_cites",
            "claim_events",
            "flags",
        ]
        .map(|t| rows(tx, &format!("SELECT * FROM {t}")))
    };
    let before = snapshot(&mut tx);
    // The same line again: present in `events`, so the fold returns.
    tx.execute("DELETE FROM flags", &[]).unwrap();
    fold(&mut tx, &added).unwrap();
    assert!(
        flags(&mut tx).is_empty(),
        "an event in `events` is not folded again"
    );
    tx.execute(
        "INSERT INTO flags VALUES ('claim', 'C2', 'promoted, not reviewed', 'No review yet', ?)",
        &[added.id.as_str().into()],
    )
    .unwrap();
    assert_eq!(snapshot(&mut tx), before);

    // Another library's event is ignored.
    let other = chain
        .line(&KnownEvent::new(EventType::FileMarkedRemovable, &json!({"file": "F1"})).unwrap());
    fold(&mut tx, &other).unwrap();
    assert_eq!(snapshot(&mut tx), before);
}

#[test]
fn claim_events_build_and_validate() {
    let claim = c2("A sentence");
    let ev = claim.added_event().unwrap();
    assert_eq!(ev.get("supersedes"), Some(&Value::Null));
    assert_eq!(ev.get("report"), Some(&json!("T1")));
    // An empty sentence is not of its form.
    assert!(c2("").added_event().is_err());
    assert!(claim::retracted_event(ClaimRef::Owner(1), "").is_err());
    // The checked body is the Checker's part plus Claims' four keys.
    let rev = EventId::parse(&format!("sha256:{}", "c".repeat(64))).unwrap();
    let ev = checked(
        ClaimRef::Owner(2),
        &rev,
        &["F1@v1".into()],
        "holds",
        Some(&["G10"]),
    );
    assert_eq!(
        ev.get("evidence"),
        Some(&json!({"outcome": "holds", "detail": null}))
    );
    assert_eq!(ev.get("against"), Some(&json!(["F1@v1"])));
    assert_eq!(ev.body().len(), 12);
}
