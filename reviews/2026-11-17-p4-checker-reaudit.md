# Re-audit: p4-checker at b1acd5e, against the 2026-11-16 design review

Scope: whether each finding of `reviews/2026-11-16-p4-checker-design-decisions.md`
is closed, rulings 54 and 55 applied where they say they land, and the
confirmations asked for. Fix-round commits: 3d4fa6b, 275474f, d76b302,
a684104, 971ca53, 7a402cd, d70c7cf, 4d4cf38, b1acd5e. Tests not run;
`cargo` is not available in this sandbox.

Rerun 2026-11-18 at b1acd5e. The one commit since 4d4cf38 adds the SQL
snapshot for `assert_names_missing_group` and touches nothing else: no
design, spec, code, or conformance file. The snapshot's SQL is the group
statement of 11.18.2 (step 0 in key order, `GROUP BY` the encoded column,
`NULLS FIRST`), consistent with rulings 50 and 55. Every finding, status,
and confirmation below holds unchanged at b1acd5e.

## Verdict

**Merge.** Nothing blocks. All three Majors are closed. One Minor is
partly open: ruling 52 lands where its text says, but rank's two key
tie-breaks still do not state where a missing key component goes. Ruling
50's rank half is stated but no case or test exercises it. Neither blocks
the merge. Both are one-line follow-ups.

## Findings

| Finding | Status | Evidence |
|---|---|---|
| M1 `expected` null for compare_rows/exists | Closed | 11.16 gives `null` for those two kinds; `claims.rs` `Form::Null(&Form::Expected)`; `run_result.rs` `every_assertion_kind_writes_a_valid_body` runs all five kinds through the body check; `claim.checked.compare_rows.valid.json` added |
| M2 `observed` tightened | Closed | 11.16 lists five forms and drops the "single exception" sentence; `Form::Observed` / `check_observed` in `events/mod.rs`; both P1 cases regenerated, the invalid one still failing only at `/body/versions/build`; the old valid line kept byte for byte as `claim.checked.bare_observed.invalid.json` (`E1002` at `/body/assertion/observed`) |
| M3 missing/nan/inf as group and rank-partition values | Closed differently but acceptably | Ruling 50 (value is `missing`, not the first member's, per the owner) lands in 11.18.2 group and rank; `run.rs` `group` maps nan and inf representatives to `missing`; case `group_missing_values_one_group`. Residual: no case or test groups a `rank` `group_by` on a missing value (see Minor below) |
| R1 min/max among equal values | Closed | Ruling 51 in 11.18.2 group Types; the code already took the first in input order; case `group_min_max_equal_values` |
| R2 missing key component in the tie-breaker | Closed for sort and Initial order; open for rank | Ruling 52 in 11.18.2 sort and Initial order; case `sort_missing_key_component_first`. My R2 also named rank's ordinal tie-break, which the rank row still leaves unstated (Minor below) |
| R3 float64 overflow | Closed | Ruling 53 in 11.7 and item 47; `tm-files` test for `1e400`, `-1e400`, and `1.5E+309`; case `float64_overflow_cell` |
| C1 11.6 DECIMAL(38, s) sentence | Closed | Now about the parse cache's `Decimal128(38, s)` and the 11.7 value-change rule |
| C2 11.8 steps null "no SQL" | Closed | Now says step 0's statement is recorded |
| C3 11.16 error-null reason | Closed | Gives both cases: no final table, or a final table that is not evidence (`E6004` at a re-check); it also states `steps` is empty, which matches `run.rs` `failed()` |
| C4 CUJ 4 "with neighbors" | Closed | Removed |
| N1 log gap-fills in the code commit | Closed | `decisions.md` entry; this round's own gap-fill (the harness turns a parse refusal into an error verdict) is logged in the commit that makes it, d70c7cf |
| N2 group statement comment | Closed | `translate.rs` writes a `-- <input> also:` line naming `key` and `d<j>`; 14 snapshots gained that line |
| N3 lowercase COLLATE | Closed | The test now also refuses `COLLATE binary` |
| N4 item 92 duplicate | Closed | The first sentence was removed |

## Minor: rank's key tie-breaks (rulings 50 and 52, in a hunk this round touched)

`translate.rs` `key_terms` puts `NULLS FIRST` on every key tie-break. That
covers `sort` and step 0, and also `rank` with `ties: ordinal` and rank's
output order ("rank ascending then key"). The 11.18.2 rank row gained a
ruling-50 sentence this round but says only "ordered by `by` then the key".
Ruling 52's text is general ("When the table key breaks a tie"). Its
landing names only sort and Initial order. P5 is written from `checker.md`,
so it has to infer rank's handling from the sort row. A group on a column
with missing values followed by a rank reaches this case, and so does
item 89's generator. Fix: add ", a missing key component sorting first"
after both mentions of the key in the rank row. Ruling 50's rank half
(missing, nan, and inf in `group_by` form one partition) is stated and
matches SQL `PARTITION BY` on the NULL encoding. No case or test covers it.
One checker case through `checker-diffs.md` (or the case writer) would
close both. [Certain for the text and code; likely that the differential
will reach it.]

## Rulings 54 and 55

- **54** lands in 11.12 (`field` is `null` for an `E3102` about a data file
  cell, and the message names the row and the column) and in 11.7 (the same
  sentence after the float64 paragraph). Both are applied. `tm-files`
  already produced field `null`. The test now pins both the field and the
  message. `docs/errors.md` is unchanged and allows `null`. [Certain]
- **55** lands in 11.9: a row component may be `missing`, and the schema
  check reads "of its column's type, or `missing`". It also lands in 11.18.3
  as a new "Locating a row" row: `missing` matches by grouping identity, not
  by `=`. Both are applied. The code matches both: `assertion.rs` accepts
  `missing` in `row`, and `expected` still refuses it; `run.rs` `same_key`
  matches missing against missing. No applied text contradicts an unnamed
  section. Cells citations are a separate form, and the gate A body forms
  already accept `missing` as a typed value. The independent case is
  `assert_names_missing_group`. [Certain]

Design before code holds this round. The order was rulings 3d4fa6b,
design 275474f, code d76b302, cases a684104; then rulings 971ca53, design
7a402cd, code d70c7cf, case 4d4cf38.

## Confirmations

- **The five new cases:** they entered in a684104 (four) and 4d4cf38 (one).
  Each commit touched only `conformance/checker/`. Since d75096a, the
  history of every `expected.json` under `conformance/checker/` shows only
  additions, so none has been modified. `checker-diffs.md` Entries is still
  "None yet". [Certain]
- **The regenerated cases cite ruling-driven regeneration:** yes. The
  `docs/decisions.md` entry "P4 review M1 and M2" (in d76b302, the same
  commit as the cases) gives the old and new IDs of both P1 cases and says
  "Ruling-driven regeneration, as for the P3 manifest cases". The cases
  were changed because the owner adopted findings M1 and M2, not a numbered
  ruling, and the entry says so. Ruling 55 regenerated no assertion schema
  case, and none needed it: no case under `conformance/formats/` puts
  `missing` in a `row`. [Certain]
- **`conformance/formats/` against main:** nine files differ. Four are
  expr cases (one renamed from `err_exponent.json`, three added). Five are
  `claim.checked` envelope cases: `error.valid` from E2, `valid` and
  `invalid` regenerated, `bare_observed.invalid` and `compare_rows.valid`
  added. There is no schema case. [Certain]
- **spec-drift at HEAD:** the `checker.md`, `formats.md`, and `record.md`
  halves pass. The `errors.md` half is **unverified**: it needs
  `TM_ERRORS_OUT` from a `cargo build`, and there is no `cargo` here.
  Neither `docs/errors.md` nor `crates/tm-formats/src/errors.rs` differs
  from main. [Certain]
- `docs/spec/record.md` is still unchanged from main. [Certain]
