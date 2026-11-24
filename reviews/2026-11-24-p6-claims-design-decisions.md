# Review: p6-claims at b9dcb51, design decisions

Reviewer session, 2026-11-24. Scope: the seven commits 24438db..b9dcb51 on
`p6-claims` (1bbce3b, the P5 merge, is the base), reviewed in the
seven-item order of the request. Tests were not run (CI is green per the
request). **[Certain]** means I read it or ran a read-only check this
session; **[likely]** is a strong inference from text or code I read. I
read `docs/design/toulmin_v1_design.md` in full at HEAD, `docs/decisions.md`
in full, `docs/design/runbook-to-p1.md` section 6, every commit's diff,
every file under `crates/tm-claims/`, the Files fold
(`crates/tm-files/src/fold.rs`) for the P3 precedent, the `claim.checked`
schema in `crates/tm-formats/src/events/claims.rs`, and the Checker's
key-map construction (`crates/tm-checker/src/run.rs`, `finish`). Read-only
checks run: my own comparison of `docs/spec/claims.md` and
`docs/spec/formats.md` with their design sections (spec-drift's
normalization re-implemented, not the CI script), and a grep of every
crate name tm-claims mentions.

## Verdict

**Merge. Nothing blocks.** The fold keeps every clause of 11.17's
contract, every 7.3 cell matches the table as ruled on 2026-11-24 and is
tested both ways, item 66's split is exactly what the code stores and
computes, the spec copies its two sections verbatim, and tm-claims depends
on no library. One Major should be settled before P7 writes its first
`claim.checked`, because after that it is in hashed bodies. It is a
one-sentence design change and no P6 code has to change:

- **M1** 11.16 orders `against` by whether the check reached a final
  table, so on an `error` verdict the fold's null-`key_map` deletion
  (11.17) lands on whichever version sorts first, not on the key-map
  version; and an `E6004` found at evaluation reads both ways.

Three Minors and six Nits:

- **m1** Item 76 (P7) still lists `claim_events` among the Core-owned
  tables; the runbook row is done in 11.17 only.
- **m2** 11.16 now states an order and "each once" for `against`;
  tm-formats checks a list of handles and nothing more, and 11.16 does not
  say the check is partial.
- **m3** The fold raises `assertion fails` only when the claim's row
  already exists. Nothing states that `claim.added` must precede
  `claim.checked`, and section 9's `draft.promote` row lists them the
  other way round.

## M1 (Major): an error verdict's `against` loses the key-map version

**Where.** 24438db: the 11.16 `claim.checked` row ("when the check has a
final table, the version its `key_map` tuples come from first ..., then
the others in 11.10 order, and otherwise all in 11.10 order") and 11.17
("the version being the body's first `against` entry ..., a `key_map` of
`null` deleting that pair's rows"). Code: `fold.rs` 284-307 deletes
`(ref, against[0])` [Certain]; `claim::against(None, ...)` sorts every
handle [Certain].

**What goes wrong.** An `error` verdict has a null `key_map` and, when the
error stopped the run, no final table, so 11.16 sorts all of `against`.
The fold then deletes the rows of whichever version sorts first, which is
the key-map version only when handle numbering happens to put it there.
Take fixture claim 4 with its tables swapped, `from: "counts"` (`F3@v1`)
joined with `de` (`F1@v1`):

1. A holding check writes `against` `["F3@v1","F1@v1"]`; `claim_keys`
   gets the `(C4, F3@v1)` rows.
2. A later `tm claim check C4` is interrupted at `checker.call_seconds`: a
   DuckDB error, so verdict `error` with `E5001` and no final table
   [likely]. 11.16 gives `against` `["F1@v1","F3@v1"]`.
3. The fold deletes `(C4, F1@v1)`, which has no rows. The `(C4, F3@v1)`
   rows stay, and `claims.citing_rows(F3@v1, ...)` keeps listing C4
   though its latest verdict records no result. With the handles the
   other way round the same error deletes them. Any multi-table claim
   whose `from` handle sorts after another cited handle is exposed, and
   binary order makes that common: `F10@v1` sorts before `F7@v1`.

The second problem: 11.16 says an `E6004` found at evaluation "leaves a
final table that is not the verdict's evidence". Whether that check "has
a final table" reads both ways, so one verdict has two `against`
encodings, two IDs, and two deletion targets. P7's `claim_check.rs`
(item 74) has to pick one when it writes the first such body.

`tests/claims.rs` does not reach this. Its error verdict (`chk4`, line
631) passes an `against` built with the key-map version first (line 612),
which is the order for a check *with* a final table, and with one version
the two orders coincide [Certain].

**Why Major.** The rule is the builder's own (24438db) and does not do
what its sentence says. The damage is bounded: stale or missing
`claim_keys` rows after an error verdict on a multi-table claim, so a
wrong "other claims citing these rows" list on the claim page. But once
P7 writes records, the fold cannot recover the key-map version from an
old error verdict. Same class as the P4 review's M2.

**Fix (my recommendation).** Order by the claim, not by the outcome: "the
`from` table's version first when the checked revision has `steps`, or
the `cells` citation's when `steps` is null and it has one, whatever the
verdict; then the others in 11.10 order; with neither, all in 11.10
order." `claim::against` needs no change (P7 passes the revision's `from`
version for every verdict), a null `key_map` then always clears the right
pair, and the `E6004` case has one encoding. The alternative, "a null
`key_map` leaves `claim_keys` as it is", also stops the arbitrary deletion
but keeps the two readings. Either way, add a two-table error case to
`tests/claims.rs` and say in the P6 decisions entry which rule holds.

## m1 (Minor): item 76 still gives `claim_events` to the Core

Runbook section 6: "Give it to the Claims fold; remove it from the Core's
list in 11.17." 24438db did that in 11.17, and item 70 and `fold.rs` write
the table [Certain]. Item 76 (design line 1567, unchanged since
`p5-reference`) still reads "`src/index/apply.rs` (runs the folds and
writes the Core-owned tables `head`, `events`, `opaque_events`,
`settings`, `audits`, `claim_events`)" [Certain]. So the design still
names two writers, and the row's failure, duplicate rows, is still where
P7 will meet it: if `apply.rs` inserts after the fold's `INSERT OR
IGNORE`, the `(ref, event_id)` key refuses the batch [likely]. Drop
`claim_events` from item 76's list; the P6 entry already says the row is
done.

## m2 (Minor): 11.16 states an `against` form the body check does not check

24438db gives `against` a form: each handle once, the key-map version
first, the rest in 11.10 order. 11.16 makes "a value not of the form this
section ... states for it" `E1002`, but
`crates/tm-formats/src/events/claims.rs:35` checks `against` as
`Form::List(&Form::Version)` only [Certain], so
`["F2@v1","F1@v1","F1@v1"]` validates. For `observed`, 11.16 says what the
body check cannot confirm; for `against` it says nothing. As with
`observed` at P4 (review M2), this is cheap before P7 writes a record;
after that, tightening the check at `tv` 1 can turn a stored line invalid. Either have tm-formats confirm "distinct, and
the entries after the first in 11.10 order" (which entry comes first is
not checkable from the body), or add a sentence that the order is the
writer's and the check confirms only the list form. Decide with M1.

## m3 (Minor): `assertion fails` needs the claim's row first

`fold.rs:308` raises only when `state_of` finds a row that is not final
[Certain]. A `claim.checked` folded before its `claim.added` raises
nothing, while item 70 says "on a claim that is neither retracted nor
superseded", which a ref with no row literally is. Gate A is safe as long
as item 75's batch keeps section 9's order (`claim.added`, then
`claim.checked`, for `claim.add` and `claim.supersede`) [likely], but
nothing written requires that order. Section 9's `draft.promote` row
(gate B) lists `claim.checked` before `claim.added`; folded in that order,
a promoted claim whose assertion fails gets no flag [Certain, from the
code]. One sentence in item 70 ("a `claim.checked` is folded after its
claim's `claim.added`; batches put them in that order"), or a raise that
does not need the row, closes it.

## Item 1: the builder's decisions without guidance

The P6 entry (24438db, 94c39b6) and the rulings entry (b9dcb51), one by
one:

- **`against` order and `claim_keys`** (11.16, 11.17). The idea is right:
  it lets the fold find the key-map version without reading the revision,
  which the contract forbids. The no-final-table branch is M1; the
  unchecked form is m2. After a `group` the key map is not keys of the
  `from` version (Nit 5).
- **7.3 column reading.** Disk differs wins over kept, a kept version
  missing from disk is in the first column, removable wins over
  fingerprint only. Consistent with the column headings, with the cells
  cell's "if kept" inside the disk-differs column, and with 6.3's "a
  deleted removable file is `expected-missing`" [Certain]. Collides with
  nothing in P7 or P8.
- **7.3 combination and `detail` clauses.** Consistent with "a claim is
  never shown as holds unless the cell says holds" and with G1. Nits 1
  and 2.
- **11.22 scanner rules.** Consistent with 11.22's first paragraph and
  D10's Pandoc spans for every case item 71 names. They cannot foreclose
  P7 or P8: no event carries spans (11.17), so the scanner can change
  later without touching the record. The two places the `spans` table
  cannot hold what the scanner returns (other attributes; two spans with
  one ref on one line) are runbook rows for P7 [Certain].
- **Item 70 rows, transitions, and flags.** Consistent with 7.1 and 5.6;
  see m3 and Nits 4 and 6.
- **Refs.** `allocate` returns the mismatch with both numbers and decides
  nothing (runbook row for P7) [Certain]. It reads the chain's last
  `claim.added` "whose ref is a `C<n>`", a narrowing of item 64 that only
  matters once gate B promotes `S<n>.<m>` claims, where it is right.
- **`checked_event`** takes the Checker's part from the caller and
  decides nothing for a claim the Checker does not run; the gap is a P7
  row.
- **The two irregular 7.3 cells** were frozen as written and reported,
  not changed; the owner ruled, and design, code, tests, and log followed
  in that order (1898aa2, 0734f11, b9dcb51). Not reopened here.
- **The five new P7 rows** each name a real gap (the `restored` column;
  the `spans` table against 11.22; `--ref` against allocation; the body
  of a Checker-less `claim.checked`; an allocation mismatch). I would
  have raised all five.

## Item 2: the fold against 11.17's contract

All [Certain], from `fold.rs` and the tests:

- **One transaction per batch.** The fold takes the caller's `IndexTx`
  and never begins or commits.
- **Idempotence by event ID.** The first statement after the type filter
  is the `events` lookup (lines 74-79). It relies on the Core writing
  `events` after the folds, as the P3 Files fold does and as item 76's
  "runs the folds and writes" orders it; the test harness does the same.
- **No record directory, no working tree.** The fold touches nothing but
  the transaction.
- **No earlier revision.** `claim.revised` is folded from its own body;
  `claim.checked` takes the key-map version from `against`, not from the
  revision it names. The fold reads `claims.state`, which item 70's
  transitions require; that is fold state, not a revision.
- **Own tables only.** It writes `reports`, `claims`, `claim_revisions`,
  `claim_cites`, `claim_keys`, `claim_events`, `verdicts`, `reviews`, and
  `flags` rows with `subject_kind` `claim`, and reads `events`. The test
  asserts that `spans` and every Files and Core table stay empty.
- **`claim_keys`.** Delete, then insert per `(ref, against[0])`, one
  canonical-JSON tuple per row; a null `key_map` deletes and inserts none
  (tested). Which pair it deletes is M1.

## Item 3: the 7.3 table

Every row, every column, and Retired are tested with the pinned test
passing and failing, and with both side marks per cell
(`tests/claims.rs` 46-139) [Certain]. I compared `cell` arm by arm with
the table at HEAD, both rulings included: no cell is decided differently
[Certain]. The reserved row's Retired cell is plain `not checkable`, and
the code gives it no `cited file retired`, as written. The only tension
is between the paragraph's "every outcome in the disk differs column" and
the reserved row (Nit 2). Every cell is reachable in gate A except
Retired (gate C) and, through gate A's CLI spelling, Reserved [likely];
both are tested anyway.

## Item 4: flags against 5.6 and item 66

The fold stores exactly `assertion fails` and `promoted, not reviewed`.
`retracted`, `superseded`, `evidence not kept`, `removable`, and
`restored` are labels nothing stores in `flags`. `text drifted`,
`evidence moved`, `expected-missing`, `possibly changed (unconfirmed)`,
and both `points at` flags are marked scan-derived for P7 [Certain]. All
42 names of 5.6 carry 5.6's kind [Certain]; the clearing-action strings
are 5.6's, two shortened (`retired`, `evidence changed since drafting`).
The stored reasons are `No review yet` (5.6) and item 70's `assertion
fails` text. One thing to know, no action: 5.6 marks `expected-missing`
"Permanent", but no event records a deletion, so a scan is its only
source and the label goes when the bytes come back. 94c39b6 names it a
label, which is the honest reading.

## Item 5: runbook section 6

- **`claim_events`** (P6 or P7): done in 11.17 and item 70; item 76 not
  updated (m1).
- **Item 66's labels** (P6): done as the row says, split by kind
  (24438db, 94c39b6) [Certain].
- **Other phases.** Outside `crates/tm-claims/` the branch changes the
  design, the runbook (five rows added), `docs/spec/formats.md`
  (regenerated for 11.16 and 11.17), `docs/spec/claims.md` (item 93),
  `Cargo.toml`, `Cargo.lock`, `deny.toml`'s allow list, and
  `docs/decisions.md`; no other crate's code [Certain]. The one reach into
  another phase's work is m2: a P6 sentence in 11.16 that P1's check does
  not follow.

## Item 6: `docs/spec/claims.md`

First line `<!-- design-sections: 7, 11.22 -->`, item 93's list. Section
7 and 11.22 at HEAD equal the file from line 2 on, byte for byte apart
from blank-line runs [Certain]. `docs/spec/formats.md` equals 11.1 to
11.17, 11.21, and 11.23 under spec-drift's normalization after 24438db
[Certain; my re-implementation, not `spec-drift.sh`]. The
`docs/errors.md` half needs `build.rs` output and was not run; CI covers
it.

## Item 7: rule 1

Dependencies: `serde_json`, `tm-formats`, `tm-index`,
`unicode-normalization`; no dev-dependencies. The only `tm_` crates named
in `src/` and `tests/` are `tm_formats` and `tm_index`, both rule 2's
shared crates [Certain]. Unlike tm-checker, it does not even take
tm-files as a dev-dependency.

## Nits

1. **No citations, `detail`.** The P6 entry says `null`; 7.3's sentence as
   committed ("`null` for `holds`, else one clause per citation that did
   not hold, ... joined by `; `") gives `""` for zero citations, a string
   11.16 accepts. The code writes `null` (`evidence_exists.rs:311`). Add
   "and for a claim with no citations" to 7.3, so a second reader of
   `claims.md` writes the same hashed body. Unreachable from gate A's CLI
   (`--cited` is required). (24438db)
2. **Reserved and `evidence moved`.** The paragraph says the flag goes
   with every outcome in the disk-differs column, which includes the
   reserved row; `cell` gives `(Reserved, DiskDiffers)` no flag, and the
   test freezes that. Item 66 makes `evidence moved` and
   `expected-missing` scan-derived per cited version, so P7's
   `read/flags.rs` will raise them whatever `Rule` says [likely];
   `Rule.evidence_moved` and `Rule.expected_missing` are a second source
   that nothing reads yet. Write "every row that names a version" in 7.3,
   and have P7 raise both from item 66's source. (24438db, 728b3c1)
3. **`verdicts.against` is JSON.** The fold stores canonical JSON there
   (`fold.rs:277`); 11.17's list of JSON-valued columns omits `against`.
   Add it. (728b3c1)
4. **`promoted, not reviewed` on withdrawn claims.** It stays open on a
   retracted or superseded claim that was never reviewed (the test
   retracts C3 unreviewed and the code leaves the flag open). That follows
   5.6's clearing column literally, but P7's `flags.list` and `tm status`
   will offer `tm claim review` as the next step for a withdrawn claim,
   and audit's `open.unreviewed` will count it [likely]. P7 should decide
   whether `read/flags.rs` hides flags of final-state claims or the fold clears it
   on retract and supersede as it does `assertion fails`. (24438db)
5. **Group key maps.** After a `group` the key map holds `by` tuples
   (11.18.2 Key mapping), which are not keys of the `from` version, so
   "the version its `key_map` tuples come from" is not true, and
   `claim_keys` files them under the `from` version. Fixture C3 groups
   `de.tsv` by `up`: `claim_keys` gets `[false]` and `[true]` under
   `F1@v1` beside gene IDs, and `claims.citing_rows(F1@v1, ["G01"])` will
   not list C3 though its `true` group aggregates G01 [likely; the stored
   tuples are certain]. This predates P6;
   P7's item 78 should decide what `citing_rows` does with a grouped
   claim.
6. **For gate B's design document.** Item 70's "(the glossary's claim
   ID)" holds for owner claims only. The glossary gives a promoted agent
   claim the ID of its `claim.proposed`, and the fold as written sets
   `claims.id` to the main-chain `claim.added` ID. Not a P7 or P8 matter.
