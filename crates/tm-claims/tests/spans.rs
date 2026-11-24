//! Item 71's span fixtures (11.22; G11): plain, escaped bracket, two
//! attributes, duplicate ref, span in fenced code (ignored), span with
//! inline code (not checkable), NFC-different but equal text,
//! whitespace-run-different but equal text, heading class capture. Also
//! the workspace fixture's `report.md`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_lines,
    reason = "a test harness: panics report setup failures"
)]

use std::path::PathBuf;

use tm_claims::drift::{Drift, drift};
use tm_claims::spans::{Span, scan};
use tm_formats::id::ClaimRef;

fn fixture(name: &str) -> Vec<Span> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/spans")
        .join(name);
    scan(&std::fs::read_to_string(&path).unwrap())
}

fn c(n: u64) -> ClaimRef {
    ClaimRef::Owner(n)
}

#[test]
fn plain() {
    let s = fixture("plain.md");
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].claim_ref, c(3));
    assert_eq!(s[0].line, 3);
    assert_eq!(s[0].text, "Eight genes are upregulated");
    assert!(!s[0].inline_code);
    assert_eq!(s[0].section_kind, None);
    assert_eq!(s[0].attributes.len(), 0);
    assert_eq!(drift("Eight genes are upregulated", &s[0]), Drift::Same);
    assert_eq!(drift("Nine genes are upregulated", &s[0]), Drift::Drifted);
}

#[test]
fn escaped_bracket() {
    let s = fixture("escaped_bracket.md");
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].text, "the value in ] brackets");
    assert_eq!(drift("the value in ] brackets", &s[0]), Drift::Same);
}

#[test]
fn two_attributes() {
    let s = fixture("two_attributes.md");
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].claim_ref, c(6));
    assert_eq!(s[0].attributes, vec![".note", "data-x=1"]);
}

#[test]
fn duplicate_ref() {
    let s = fixture("duplicate_ref.md");
    assert_eq!(s.len(), 2);
    assert!(s.iter().all(|x| x.claim_ref == c(2)));
    assert_eq!(s.iter().map(|x| x.line).collect::<Vec<_>>(), vec![1, 3]);
}

#[test]
fn fenced_code_is_ignored() {
    let s = fixture("fenced_code.md");
    assert_eq!(s.len(), 1, "{s:?}");
    assert_eq!(s[0].claim_ref, c(7));
    // The `~~~` on line 9 is shorter than its `~~~~` opener and does not
    // close it; line 10 does.
    assert_eq!(s[0].line, 12);
}

#[test]
fn inline_code_is_not_checkable() {
    let s = fixture("inline_code.md");
    assert_eq!(s.len(), 1);
    assert!(s[0].inline_code);
    assert_eq!(
        drift("The `padj` column is below 0.01", &s[0]),
        Drift::NotCheckable
    );
    assert_eq!(drift("anything else", &s[0]), Drift::NotCheckable);
}

#[test]
fn nfc_different_but_equal() {
    let s = fixture("nfc.md");
    assert_eq!(s.len(), 1);
    // The report spells the second é decomposed; the sentence is NFC.
    assert_ne!(s[0].text, "Caf\u{e9} r\u{e9}sum\u{e9} holds");
    assert_eq!(
        drift("Caf\u{e9} r\u{e9}sum\u{e9} holds", &s[0]),
        Drift::Same
    );
}

#[test]
fn whitespace_run_different_but_equal() {
    let s = fixture("whitespace.md");
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].claim_ref, c(10));
    assert_eq!(drift("Six samples were sequenced", &s[0]), Drift::Same);
    assert_eq!(drift("Six samples were sequenced.", &s[0]), Drift::Drifted);
}

#[test]
fn heading_class_capture() {
    let s = fixture("heading_class.md");
    let kinds: Vec<(ClaimRef, Option<&str>)> = s
        .iter()
        .map(|x| (x.claim_ref, x.section_kind.as_deref()))
        .collect();
    assert_eq!(
        kinds,
        vec![
            (c(1), None),
            (c(2), Some("results")),
            (c(3), None),
            (c(4), Some("methods")),
        ]
    );
}

#[test]
fn workspace_report() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/report.md");
    let s = scan(&std::fs::read_to_string(path).unwrap());
    assert_eq!(
        s.iter().map(|x| x.claim_ref).collect::<Vec<_>>(),
        (1..=10).map(c).collect::<Vec<_>>()
    );
    assert_eq!(s[9].text, "The treated sample from batch 2 is S5");
    assert!(s.iter().all(|x| x.section_kind.is_none() && !x.inline_code));
}
