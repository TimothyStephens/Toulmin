# Re-audit: p6-claims at 1088b91, against the 2026-11-24 design-decisions review

Reviewer session, 2026-11-24. Scope: whether each finding of
`reviews/2026-11-24-p6-claims-design-decisions.md` (reviewed at b9dcb51)
is closed, plus the two checks the request names, over the nine commits
b9dcb51..1088b91: 1af1ae7 (the review itself, byte-identical to the file
I wrote); 8ebb5d4 and 5169215 (M1); 8003497 (m1); 21e2c52 and 029b783
(m2, option (a)); 59a9ec2 (m3); f3f2f1e (nits 1 to 3); 1088b91 (nits 4 to
6). No test suite was run, per the request, and nothing was built. What I
did run, all read-only: a word-level diff of every design line changed
since b9dcb51; my own comparison of `docs/spec/claims.md` and
`docs/spec/formats.md` with their design sections (spec-drift's
normalization re-implemented, not `spec-drift.sh`); a `hashlib` check of
the two new envelope lines' IDs and canonical form; greps for every
conformance case and every workspace producer of a `claim.checked` body.
**[Certain]** means I read it or ran a read-only check this session;
**[likely]** is a strong inference from code I read but did not run.

## Verdict

**Merge. Nothing blocks.** All ten findings are closed, each as the owner
accepted it: the Major and the three Minors by design edits (and, for M1
and m2, code and tests), nits 1 to 3 by design sentences, and nits 4 to 6
as runbook rows, which is where the accepted ruling put them. No frozen
case under `conformance/formats/` changed bytes, ID, or validity, and the
M1 test uses a `from` version that sorts second. The design diff since
b9dcb51 is five lines, each exactly the accepted text, and both affected
specs match their sections at HEAD.

## Findings

| Finding | Status | Evidence |
| --- | --- | --- |
| M1 an error verdict's `against` loses the key-map version | Closed | 8ebb5d4: the 11.16 row (design 939) now orders `against` by "the checked revision's `from` table's version first when the revision has `steps`, or its `cells` citation's version when `steps` is null and it has one, whatever the verdict (`error` included); then the others in 11.10 order; with neither, all in 11.10 order". The "when the check has a final table ... otherwise" branch is gone, so an `E6004` found at evaluation has one encoding. 11.17's `claim_keys` sentence is unchanged. 5169215: `claim::against` keeps its signature, and its doc comment states the revision rule (`claim.rs` 327-334); the regression test is below. Decisions entry at 1485. [Certain] |
| m1 item 76 gives `claim_events` to the Core | Closed | 8003497: item 76 (design 1567) now lists `head`, `events`, `opaque_events`, `settings`, `audits`. The only remaining mentions of `claim_events` are the schema, 11.17's "(`claim_events` is the Claims fold's, item 70)", and item 70. Entry at 1491. [Certain] |
| m2 `against` form unchecked by tm-formats | Closed, option (a) | 21e2c52: 11.16 says the body check confirms the handles are distinct and the entries after the first are in 11.10 order, `E1002` at `/body/against/<i>`, and cannot confirm which comes first. 029b783: `check_against` (`events/mod.rs` 291) runs for `claim.checked` after `check_object` passes (line 228, the same place as the `file.parse_spec_set` fingerprint check); it checks repeats from index 1 and order from index 2, and has a unit test with a repeated handle. There are two new envelope cases (below). Entry at 1497. [Certain] |
| m3 `assertion fails` needs the claim's row first | Closed | 59a9ec2: item 70 now ends "A `claim.checked` is folded after its claim's `claim.added`, and a batch puts them in that order." Runbook section 6 line 149 is the gate B row for `draft.promote`'s order. Entry at 1503. [Certain] |
| Nit 1 `detail` for no citations | Closed | f3f2f1e: 7.3 now reads "`evidence.detail` is `null` for `holds` and for a claim with no citations", which is what `evidence_exists.rs:311` writes. [Certain] |
| Nit 2 reserved row and `evidence moved` | Closed | 7.3 now reads "every outcome in the "disk differs" column of every row that names a version", which agrees with `cell` and the test leaving the reserved row unflagged. [Certain] |
| Nit 3 `verdicts.against` is JSON | Closed | 11.17's JSON-valued column list (design 999) now includes `against`. [Certain] |
| Nit 4 `promoted, not reviewed` on withdrawn claims | Closed, as a runbook row (as ruled) | 1088b91: runbook line 150, P7, both options named. [Certain] |
| Nit 5 group key maps in `claim_keys` | Closed, as a runbook row (as ruled) | Runbook line 151, P7, item 78's `claims.citing_rows`. [Certain] |
| Nit 6 claim ID of a promoted agent claim | Closed, as a runbook row (as ruled) | Runbook line 152, gate B design. [Certain] |

All six decisions entries (1485 to 1515) were added in the same commit as
their design edit; 5169215 and 029b783 carry code only and point back to
theirs. [Certain]

## Check 1: no frozen case under `conformance/formats/` changed

All [Certain]:

- **Bytes and IDs.** `git diff --name-status b9dcb51 HEAD -- conformance/`
  lists exactly two added files,
  `envelope/claim.checked.against_order.valid.json` and `.invalid.json`,
  and no modified or deleted file. An ID is the hash of a line's bytes, so
  every existing case keeps its ID.
- **Validity.** The new check runs only for `claim.checked` and reads only
  entries from index 1 on. All five existing `claim.checked` cases
  (`valid`, `compare_rows.valid`, `error.valid`, `invalid`,
  `bare_observed.invalid`) carry `against` `["F7@v2"]`, so the check cannot
  refuse one. The two existing invalid cases fail inside `check_object`
  (`/body/versions/build`, `/body/assertion/observed`), which runs first,
  so their reported fault is unchanged.
- **The new cases.** Valid: `["F7@v2","F1@v1","F2@v1"]`. Invalid:
  `["F7@v2","F2@v1","F1@v1"]`, expecting `E1002` at `/body/against/2`.
  Both lines are canonical and carry the ID their bytes give, and the
  valid case's `expect.id` matches it. The invalid line differs from the
  valid one only in `against`, and the valid line differs from the frozen
  `claim.checked.valid.json` only in `against`. Python's envelope test reads a
  body-fault case at the line level only, so it should accept both
  [likely].
- **Nothing else stored is refused.** `conformance/record/` holds no
  `claim.checked` line. The only other workspace code that builds a
  `claim.checked` body, `tm-checker/tests/run_result.rs`, passes a
  one-entry `against`.

## Check 2: the M1 test uses a `from` version that sorts second

`an_error_verdict_clears_the_from_version_of_a_two_table_claim`
(`tests/claims.rs` 782) declares `{"tables": {"counts": "F3@v1", "de":
"F1@v1"}, "from": "counts"}` with a join on `de`. The `from` version
`F3@v1` sorts after `F1@v1` in 11.10 order [Certain]. The test asserts:

- `against` is `["F3@v1","F1@v1"]`;
- a holding check writes two `(C4, F3@v1)` rows;
- the error body has `key_map` null and the same `against`;
- after the error the claim has no `claim_keys` rows.

Under the replaced rule, the error's `against` would have been
`["F1@v1","F3@v1"]` and the `F3@v1` rows would have stayed. So the test
pins exactly the case M1 named [Certain, by reading; not run].

## Scope and what I did not do

Nothing outside the nine commits was re-reviewed, and inside them I found
nothing new. Five design lines changed since b9dcb51 (621, 939, 999,
1558, 1567), each exactly the accepted ruling. The old wording of the
`against` rule survives only in the M1 entry that quotes what it
replaced. I did not build or run anything. The new tm-formats test module
and its helper paths (`errors::ptr`, `sort_order::cmp_text`) resolve on
reading, and the file has one test module, but compilation is CI's to
confirm. `docs/errors.md` is untouched, and no catalog code changed.
