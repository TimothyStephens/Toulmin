# Review: p4-checker at d87ab4e, design decisions

Reviewer session, 2026-11-17 (file named for the review request of
2026-11-16). Scope: the design decisions in the 27 commits d1515c1..d87ab4e
(16dc0d0 through d87ab4e; d1515c1 is the P3 merge and is the base), not code
quality. The test suite was not run, and this sandbox has no `cargo`, so
nothing was built. **[Certain]** means I read it or ran a read-only check
this session; **[likely]** is a strong inference from code I read but did
not run. I read `docs/design/toulmin_v1_design.md` in full at HEAD, then
`docs/decisions/checker-spec-review.md`, `docs/decisions/checker-diffs.md`,
every P4 entry of `docs/decisions.md`, and each commit's diff in order.
Code read: `tm-checker`'s `table.rs`, `translate.rs`, `run.rs`, `eval.rs`
(context and `fix`), the conformance harness, the SQL snapshots, the
`claim.checked` form in `tm-formats`, and the float64 and dec paths of
`tm-files/src/parse.rs`.

## Verdict

**Merge. Nothing blocks.** The key encoding holds both properties for every
pair asked about, every `expected.json` is the case writer's and unchanged
since, and the 49 rulings are applied where the sign-off says. Three Majors
should be fixed before P5 starts and before P6 writes its first
`claim.checked`; none of them silently mis-groups or mis-sorts, and each is a
small design-first change:

- **M1** the Checker writes `"expected": null` for `compare_rows` and
  `exists`, which the 11.16 body check refuses and 11.16 gives no form for.
- **M2** (your item 9) tighten `assertion.observed` now; nothing argues
  against it, but the frozen *invalid* case must be regenerated too.
- **M3** how `group` and `rank` `scope: group` treat missing, `nan`, and
  `inf` `by` values lives only in Rust; P5's differential will hit it.

## M1 (Major): `compare_rows` and `exists` verdicts write a body the 11.16 check refuses

**Where.** `crates/tm-checker/src/run.rs` `finish`: the `CompareRows` and
`Exists` arms set `expected_json` to `Value::Null`, and `Verdict::
body_members` writes it as `assertion.expected`. `tm-formats/src/events/
claims.rs:51` checks that member with `Form::Expected`, and
`check_expected` (`events/mod.rs:429`) accepts only a typed value or a
two-element list; `null` falls to `Form::Typed`, which refuses it.
[Certain] for the code paths; [likely] for the failure, since I did not run
it.

**The design gap.** 11.16 says `expected` "is the 11.9 `expected` shape (a
typed value, or `[lo, hi]`)". 11.9 gives `compare_rows` and `exists` no
`expected` member. So for two of the five assertion kinds there is no legal
value, and the Checker's choice of `null` is refused.

**Why it was not caught.** 8d90531's `run_result.rs` passes a body through
the `tm-formats` check (`checked_body`) for one `value` holds verdict and one
validation error. No `compare_rows` or `exists` body is ever checked, and
the envelope cases have only the `value` shape.

**Consequence.** Fixture claims 4 (`compare_rows`) and 5 (`exists`) in item
82a produce a `claim.checked` that P6's write path refuses with `E1002` at
`/body/assertion/expected`. It fails loudly, not silently, and the fix
only loosens the format, so it could wait. But it belongs in the M2 change,
which edits the same row and the same form.

**Fix.** 11.16: "`expected` is the 11.9 `expected` shape (a typed value, or
`[lo, hi]`), or `null` for `compare_rows` and `exists`, which have none." In
`claims.rs`, `("expected", Form::Null(&Form::Expected))`. Add
`run_result.rs` tests that put one verdict of each of the five kinds through
`checked_body`. Add a valid envelope case with a `compare_rows` body.

## M2 (Major, your item 9): tighten `assertion.observed` now

Nothing argues against it, and three things argue for it:

- The P1 eval cases already use the per-kind form. Every
  `assertions/eval/*/case.json` carries `observed` as `{"cell"}`,
  `{"left","right"}`, and so on, and `evaluate_located` reads that form
  [Certain]. So the bare typed value in `claim.checked.valid.json` is the
  only outlier in the repository.
- No record exists until P7. After that, tightening at `tv` 1 is impossible
  and the open form is permanent.
- The precedent is the P3 manifest case: a ruling-driven regeneration of a
  frozen case, named in the commit and `docs/decisions.md`.

Three things the fix must get right:

1. **The frozen invalid case changes too.**
   `conformance/formats/envelope/claim.checked.invalid.json` also carries
   `"observed":{"t":"dec","v":"0.00310"}`, and its expected error is
   `/body/versions/build`. `assertion` precedes `versions` in the
   `claim.checked` form list, so once `observed` is checked the first error
   becomes `/body/assertion/observed` [likely], and the case stops testing
   what it says. Regenerate it with a per-kind `observed` so its one fault
   stays `build`.
2. **The check can enforce only "one of the five forms".** The assertion's
   kind is in `claim.added`/`claim.revised`, not in `claim.checked`, so the
   body check cannot tell whether the form matches the kind. The forms are
   distinguishable by their keys, so a union check is exact for what it can
   see. 11.16 should say so in one clause, so P6 (which has both events)
   knows the cross-check is its job, if anyone's.
3. **Do M1 in the same commit.**

**Fix.** 11.16: drop the "single exception" sentence and give `observed` a
form like any other member. The form is one of `{"cell": typed value or
null}`, `{"position": integer ≥ 1 or null}`, `{"count": integer}`,
`{"left","right"}` (each a typed value or null), or `{"present": boolean}`.
In `claims.rs`, a `Form::Observed` replaces `Form::Any`. Regenerate
`claim.checked.valid.json` (`{"cell":{"t":"dec","v":"0.00310"}}`) and
`claim.checked.invalid.json`, and update their `expect.id`. Add one invalid
case with a bare typed value at `/body/assertion/observed`. Record it in
`docs/decisions.md`, citing this item as the ruling and naming both
regenerated cases.

## M3 (Major): missing, `nan`, and `inf` as `group` and `rank`-partition values live only in Rust

**Where.** `translate.rs` `Op::Group`: `GROUP BY "c<j>"`. `Op::Rank` with
`scope: group`: `PARTITION BY "c<j>"`. Every missing, `nan`, `+inf`, and
`-inf` cell encodes as SQL `NULL` (`table.rs` `order_key`), and SQL puts
every `NULL` in one group. `run.rs` `group` then takes the group's `by` value
from its first member (`rep`), which can be `{"t":"nan"}` or
`{"t":"inf","s":"-"}`. [Certain]

**What `checker.md` says.** 11.18.2 `group`: "Two `by` values fall in one
group when they are equal under the `=` rule", and "a group with a missing
component sorts before the groups with a value there". Under the Missing row,
`missing = missing` is `missing`, not `true`, and `nan` and `inf` are
`missing` for every operator. Read literally, no two missing rows are equal,
so each would be its own group, and the output key would no longer be
unique. Read by analogy with `rank` ("two rows whose `by` values are both
missing tie"), they form one group. But then whether `nan`, `inf`, and
`missing` form one group or three is still open. Rust picks one group.
`rank` `scope: group` has the same gap for `group_by`.

**Why Major.** No conformance case groups on a column with a missing value
[Certain]: every `group_*` case groups on `tissue`, `grp`, `sample`, or
`cond`, none missing. Item 89's generator covers "missing and NaN values" in
every op, so `differential.sh` will meet this on its first run and differ in
`final_rows` and both hashes. P5 is written from `checker.md` alone.

**Fix (one sentence each, 11.18.2).** In `group`: "All rows whose `by` value
is `missing`, `nan`, or `inf` in a component fall in one group for that
component, and the group's `by` value is its first member's, as written
(`nan`, `inf`, or `missing`)." Say the same of `rank`'s `group_by` partitions.
Add one checker case for each to `conformance/checker/cases/` through a
`checker-diffs.md` entry, since the case writer's pass is closed.

## The other Rust-only behaviours (item 6, for P5)

Each one lives in code and not in `checker.md`. Each is a likely
differential difference, and each is a one-clause fix.

- **R1 (Minor). `min` and `max` among equal values of different scale.**
  `translate.rs`: `first(row ORDER BY key ASC|DESC, row)` returns the earliest
  input row among equal extremes, so a derived `dec` column holding `1.5` then
  `1.50` gives `1.5`. 11.18.2 says only "keep it". Python's `min` and `max`
  also return the first of equals, but `sorted(...)[-1]` returns the last.
  Add "among equal values, the first member's in the input order, as for the
  `by` value."
- **R2 (Minor). Key tie-breaker with a missing key component.** After a
  `group` on a column with missing values, the output key can hold
  `missing`. `key_terms` places it `NULLS FIRST` for the `sort` and
  `rank ordinal` tie-breakers and for step 0. 11.18.2 says only "the table
  key is appended ascending". Add "missing key components first, as `group`
  orders them."
- **R3 (Minor). A `float64` cell that overflows to infinity.**
  `tm-files/src/parse.rs:395`: `1e400` parses to `inf` and is `E3102` "not
  finite". 11.7 says any other text "is read as an IEEE double ... and stored
  shortest-round-trip", which gives no answer for overflow. Python's
  `float('1e400')` is `inf`. Add "a text whose double is not finite is
  `E3102`."

## Item 1: the 49 rulings

Every ruling is applied in the section the sign-off file names [Certain]. I
checked each against HEAD text, not only against the diffs. The pairs you
singled out:

- **5 and 39.** 11.6's `dec` row ("written at its column's `scale`"), 11.7
  ("`E3102` only when writing it at the column's scale would change its
  value", with the `0.001`/`0.0010`/`1.0e-3`/`0.0011` example), and item 47
  agree. 43's shown-scale rule sits beside them without conflict. **One
  sentence ruling 39 did not name still reads the old way (C1, below).**
- **16 and 35.** 11.6, 11.18.2 Precision, and Domain state the same context
  (34, `Emax` 999999, `Emin` -999999, half-even), with Overflow as `E5002`
  only and underflow going subnormal, then to zero. No sentence still says
  overflow happens at 34 digits. `derive_overflow_emax` and
  `derive_rounds_at_34_digits` carry the re-derived reading. The Rust side
  checks `Emax`/`Emin` itself: `eval.rs` `fix` is Python's `_fix` with
  `ETINY`, `ETOP`, and the zero-exponent clamp, not `bigdecimal`'s context
  [Certain].
- **20 and 23.** 11.9 and 11.18.3 both say a *present* `tol`, zero or not,
  on a non-number is `E6004`. The rank assertion is the 1-based position in
  11.9's table, 11.18.3, and 11.16's `{"position"}`. CUJ 6 and fixture
  claim 1 still read correctly under position.
- **32 and 36.** 11.18.1 `EXPNUM`, the unary-minus rule, the range check, and
  48's quantize-outside-the-context sentence. 11.6's exponent-input
  paragraph. 11.7's cell rule. 11.20's display sentence. `typed_values/
  dec_exponent` is unchanged [Certain]. The builder's reading that 36 covers
  plain `INT` and `DEC` literals is in 11.18.1's text, so P5 sees it.

**Contradictions with sections the rulings did not name:**

- **C1 (Minor), 11.6 last paragraph:** "Decimal columns map to DuckDB
  `DECIMAL(38, s)` ...; a value that does not fit at that scale is a parse
  failure (`E3102`)." This sentence predates P4. "Does not fit at that scale"
  reads as "more fraction digits than `s`", which is the rule ruling 39
  replaced. "Map to DuckDB `DECIMAL`" also reads against E2's Where row ("No
  value enters as a DuckDB `DECIMAL`"). It is in `checker.md`, so P5 reads
  both sentences. Reword: "In the parse cache a `dec` column is Arrow
  `Decimal128(38, s)` (item 47); a cell that needs more than 38 digits at
  scale `s`, or whose value changes when written at `s` (11.7), is `E3102`."
- **C2 (Minor), 11.8 against 11.18.2 Where:** 11.8 says a claim with `steps`
  null has "no pipeline and no SQL". The Where row and the code record step
  0's statement: `cells_rows` runs `step0` in DuckDB, and the stored `sql` is
  non-empty. Pick one. Recording step 0 is consistent with the Where row and
  costs nothing.
- **C3 (Minor), 11.16:** the error-null sentence says the error verdict
  "stops before a final table exists". An evaluation-time `E6004` (a
  re-check whose column type changed, 11.9) has a final table, and the code
  still nulls both hashes and `key_map` and drops `steps`. This agrees with
  item 92 and with the body check, which accepts null "whatever the
  outcome", but the stated reason is wrong for that case. Say "on a Checker
  verdict of `error`, these three are `null`."
- **C4 (Minor), CUJ 4:** it still describes "the cited rows highlighted in
  their full rows with neighbors". Ruling 38 kept the removal of "with
  neighbors" in section 10 and 11.20, but not here. A P8 builder reads CUJ 4.

## Item 2: `checker-diffs.md`

There are no entries, so nothing resolves a disagreement by changing an
`expected.json`. The 28 eval pairs are tabulated as agreeing. I spot-checked
the error rows against `expected.json` and the file's own reading rule for
`observed.p1.json`, and they match [Certain]. `tests/conformance.rs` has
`OPEN = &[]` and fails if a listed case starts agreeing, so the file and the
test cannot drift apart silently. **No `expected.json` changed after it
entered, so there is no Blocker under this item.**

## Item 3: independence

- All 152 `expected.json` under `conformance/checker/` were added in
  d75096a, and no later commit touches `conformance/checker/` at all
  [Certain] (`git log d1515c1..HEAD -- conformance/checker`).
- d75096a touches only `conformance/checker/` (663 files) [Certain]. The
  first commit that touches `crates/tm-checker/` is 3accbb9, 2 h 25 min
  later [Certain]. Before d75096a, `tm-formats` gained `Lit::value`
  (99131d8, ruling 45). That is shared-formats code, not Checker code.
- `observed.p1.json` was added in P1 (4352d5f). `write_cases.py` never
  references `observed` [Certain]. The harness reads only `expected.json`
  (`tests/common/case.rs:147`). `examples/refcheck.rs` writes
  `observed.json`, never `expected.json`. The two files differ in shape
  (`{outcome}` against `{verdict, error}`). I cannot see the case writer's
  prompt, so whether it was told not to open `observed.p1.json` (runbook
  section 6, row 22) is [likely], from that evidence and d75096a's message,
  not [Certain].

## Item 4: the key encoding

`table.rs` `number_key`: zero is `1`. A positive value is `2`, then
`10^18 + E` as 19 digits, then the significant digits with trailing zeros
stripped, where `E` is the exponent of the value written `0.d1…dn × 10^E`.
A negative value is `0`, then `10^18 − E` as 19 digits, then the nines'
complement of the digits, then `~`. `str` passes as itself, `date` as
`YYYY-MM-DD`, `datetime` as `YYYY-MM-DDTHH:MM:SS.nnnnnnnnn`, `bool` as
`BOOLEAN`, and missing, `nan`, and `inf` as `NULL`. Walked [Certain]:

| Pair | Equal under `=` gives identical keys | Unequal values order as 11.18.2 |
| --- | --- | --- |
| `int` 2 against `dec` 2.0 | Both reach `BigDecimal` and give `E=1`, digits `2`: identical | n/a |
| `1.5` against `1.50` | `(15,1)` and `(150,2)` give `E=1`, digits `15`: identical | n/a |
| Negative zero | `-0.00` has a zero coefficient, so the key is `1`, as for `0` | `0…` < `1` < `2…` |
| Negatives of differing magnitude | n/a | Larger `E` gives a smaller `10^18−E`, so it sorts first (−12 before −1.25). Same `E` with a digit prefix: `-0.151` gives `848~` and `-0.15` gives `84~`; `8` (0x38) < `~` (0x7E), so −0.151 sorts first. Correct, and the `~` is what makes it so |
| Strings differing only in normalization | Every `Str` is built through `str_nfc` (Arrow read, literals, JSON read), so the key is the NFC text: identical | Byte order under `COLLATE "binary"`, which the `translate.rs` test checks with ASCII and `é` |
| `date` against `datetime` | A column has one `Tag`. `coalesce` requires one type. A `join` `on` pair of `date` and `datetime` is `E6004` in `steps.rs` (`comparable`). They never share a column or a comparison | Fixed-width forms, so lexical order is chronological within each type |
| `bool` | `BOOLEAN` | `false` < `true` |

Positive keys with the same `E` compare digit strings lexically, and a
prefix sorts first, which is right for positives (0.15 < 0.151). The
19-digit exponent field holds any `E` within ±10^18, far beyond the context
and any `int` that fits in memory. **No pair breaks either property.**

**The stored `sql` [Certain, from snapshots and `run.rs`]** is step 0's
statement over `input.<name>`, then one statement per relational step over
`step<i>`. Each is headed by `-- <table>: c0 = "name" (type), …; key …`, and
`filter` and `derive` appear as `-- ops/<i> filter: evaluated in Rust over
step<i>; its rows are step<i+1>`. This is what 11.18.2 Where now says. Nit
N2 covers the header not naming the `key` and `d<j>` extra columns that
`group` registers.

## Item 5: the result type (item 55; rulings 33, 34, 38, 40)

`RunResult` [Certain] carries:

- `steps: Vec<StepTable>`: step 0, then one per op, each with columns, key,
  the first `limit` rows, and `row_count`. On a runtime error it holds the
  steps before the error. With `steps` null, step 0 is the cited rows and is
  the final step.
- `emphasized_rows`: role (`row`, `left`, `right`), key, and 1-based
  position or `None`.
- `beyond_cap`: role, position, `of`, and the row.
- `emphasized_columns`: the assertion's column or columns, then the `cells`
  citation columns, deduplicated and kept only if the final table has them.
- `final_columns` and `final_rows`, the latter when uncapped.
- The `Verdict`, whose `key_map` gives every final row's input keys.

That is everything section 10's `claims.evidence` and 11.20's claim page
ask for, so P7 and P8 can build both from it without changing `tm-checker`.
Two notes for P7, not defects: the type is not `Serialize`, so P7 defines
the query's JSON. `claims.evidence` re-runs the check with the caller's
`limit`, which 11.17 already says.

**`claim.checked` body.** Same twelve keys and `tv` 1. Item 55's tables
never enter it [Certain]. The only change is E2's loosening (item 8).

## Item 6: the builder's decisions made without guidance

| Decision | Consistent | Forecloses a later phase |
| --- | --- | --- |
| Ruling 34's result type in item 55, not 58 (owner accepted, 40) | Yes | No. It keeps 11.16 stable |
| Sections 9 and 10 both edited for `claims.evidence` (40) | Yes | No |
| Item 47's exponent exception (40) | Yes | No |
| Ruling 36 covers every numeric literal | Yes; in 11.18.1 | No |
| Ruling 38 keeps `cells` citation columns emphasized | Yes; in 11.20 and section 10 | No |
| CSV exponent past 38 digits is `E3102` (40) | Yes, through C1 once reworded | No |
| Order keys, one statement per step, `sum` in Rust (moved into the design in 5ce3cfc) | Yes; C1 is the one sentence left behind | No. The SQL is not in the differential, and the results are what 11.18.2 states |
| `tm-files` as a dev-dependency only | Yes; 4.2 rule 1 governs run time | No |
| 46681ce's four gaps: `join` keeps left order; `group` by `=`, first member's value, `sum` in input order; `false` before `true`; error order | Yes; all four are in `checker.md` | No |
| `claim.checked` nullable at `tv` 1 | Yes (item 8) | No |
| `observed` left open | See M2 | Yes, after P7. Hence M2 |

The Rust-only behaviours for P5 are M3, R1, R2, and R3. C2's step-0 SQL also
lives only in code, but `sql` is not in `expected.json` or the differential,
so it cannot fail P5.

## Item 7: runbook section 6, the P4 rows

| Row | Done |
| --- | --- |
| 11.18.4 column placeholders lack a backtick rule | Yes, 6da8810. Every column placeholder follows the `{expr}` rule [Certain] |
| Fuzz job on `pull_request` | Yes, a14d2d8. `schedule` or `workflow_dispatch` only [Certain] |
| The third session must not open `observed.p1.json` | [likely], item 3 |
| `COLLATE BINARY` refused (row b) | Yes, 46681ce. 11.10 states `COLLATE "binary"`, says why, and names the test. `translate.rs` proves the parser refuses `COLLATE BINARY` and that the quoted form orders by bytes [Certain]. Nit N3 |

## Item 8: 11.16 `claim.checked` changes from E2

`final_rows_hash`, `key_map_hash`, and `key_map` became `Form::Null(...)`,
and `observed` stays `Form::Any`. Both only loosen the format, so no frozen
valid case can become invalid [Certain]. No frozen invalid case relies on
those three being non-null either: the only `claim.checked` invalid case
fails on `/body/versions/build` [Certain]. The new
`claim.checked.error.valid.json` is an added case, not a changed one.

## Confirmations

| Check | Result |
| --- | --- |
| Every design edit precedes the code that depends on it | **Not as stated.** Each ruling's design commit precedes its code. But three behaviours went code-first as recorded deviations in 3accbb9 and were moved into the design in 5ce3cfc: order keys, one statement per step, and `sum` in Rust. Two more went code-first with no entry until 5ce3cfc: `steps` null as step 0, and emphasized columns limited to those present (3accbb9, b617d20). Nothing went unrecorded in the end. N1 |
| spec-drift at HEAD | `checker.md`, `formats.md`, and `record.md` OK [Certain, run here]. The `errors.md` half is **unverified**: it needs `cargo` for `TM_ERRORS_OUT`. Neither `docs/errors.md` nor the catalog changes on this branch [Certain] |
| `record.md` unchanged | Holds [Certain] |
| `formats.md` changed only in 11.6 to 11.9, 11.12, 11.16 | **Also 11.10**, the COLLATE sentence from row b [Certain]. Expected, and your list omits it; not a defect |
| `conformance/formats/` changes | Exactly the four `expr` cases: `err_exponent` renamed to `literal_exponent` (now valid), `literal_exponent_unary_minus` (32), `literal_exponent_overflow` and `_underflow` (36). Plus the one added `claim.checked.error.valid.json` (E2) [Certain] |
| Rust checks `Emax`/`Emin` itself | Holds [Certain], item 1 |

## Nits

- **N1** Process: when a builder decision fills a design gap during coding
  (steps-null step 0, emphasized-column filtering), log it in
  `docs/decisions.md` in the code commit, as the other three were.
- **N2** `translate.rs` `header`: the `group` statement's comment does not
  name the `key` and `d<j>` columns that 11.18.2 Where says "besides the
  cells" are registered. Add them, so the comment names every encoded
  column.
- **N3** 11.10 says DuckDB refuses lowercase `COLLATE binary`, but the test
  only checks `COLLATE BINARY`. Add the lowercase assertion or drop the
  claim.
- **N4** Item 92 states "on error, row-level fields are null" twice (rulings
  22 and 37 both landed). Keep the second, fuller sentence.
