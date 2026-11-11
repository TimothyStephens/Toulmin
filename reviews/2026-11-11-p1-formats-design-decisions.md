# Review: PR #1 (p1-formats), design decisions in 40f1f35, d6dd8c4, 742e0ae

Reviewer session, 2026-11-11. Scope: design decisions only, not code quality.
The test suite was not run. Evidence tags: **[Certain]** means I read it or ran
a read-only check this session; **[likely]** means a strong inference.

Design text is quoted from `toulmin_v1_design.md` **at 742e0ae**, because at the
branch head the agent's design edits are gone (finding B1).

---

## Blockers

### B1. At the branch head the design no longer says what the 254 cases freeze

**Where:** fc0ac29 ("docs: restore the full runbook-to-p1.md"). This commit is
out of scope for this review, but it decides whether the PR can merge.

**Finding [Certain].** fc0ac29 changes `docs/design/toulmin_v1_design.md` as
well as the runbook (+25/−36 lines). It replaced the design with an older copy
that predates 40f1f35. `git diff 3f45cc6 HEAD -- docs/design/toulmin_v1_design.md`
shows a single changed line (the snapshot `ino` note). Every pin from 40f1f35
and d6dd8c4 has been reverted: I-JSON and `E1003`, envelope validation, typed-value
lexical forms and `E1004`, parse-spec `type` and `scale` and `E3105`, the
op object form, `E6005` and `E6006`, the 11.16 body rules, reserved words, the
11.18.2 type rules, the 11.18.3 tolerance and `between` rules, the 11.23 CLI
grammar and `E6007`, and the item 17 and item 20 and section 9 edits.

Consequences:
- `scripts/ci/spec-drift.sh` fails at HEAD. I ran it: `FAIL docs/spec/formats.md:
  differs from design sections 11.1 … 11.23`. If CI is green, the green run was not
  on fc0ac29 [likely].
- The runbook's rule ("a conformance case freezes a format; the design is where
  a format is decided") is broken on the branch. All 254 cases, `docs/spec/formats.md`
  and `docs/errors.md` now encode formats the design does not state.
- Readiness findings the agent closed in the text are open again at HEAD
  (section 5 below).

**Fix.** Restore the design from 742e0ae. Then reapply fc0ac29's one intended
change by hand, but only after B2 is decided. In the same change, re-run
spec-drift and the formats suite.

### B2. Snapshot `ino`: decision 10, the new E1001 rule, the design at HEAD, and a frozen case disagree

**Where:** 11.1 hunk in 40f1f35 (an integer above 2^53 is `E1001`); 11.16
`snapshot` row; `crates/tm-formats/src/events/mod.rs:354` (`("ino", Form::Int)`);
`conformance/formats/envelope/file.version_added.valid.json` (`"ino": 4211`).

**Finding [Certain].**
- `decisions-p1.md` decision 10 (settled) makes `ino` a JSON integer.
- The agent's 11.1 pin makes any integer above 2^53 a hard `E1001`.
- Together, a snapshot of a file on Lustre, GPFS, or XFS with `inode64`
  (inode numbers above 2^53) cannot be serialized at all. The design targets
  exactly those filesystems (Problem section; Setup 2).
- So P2 (`tm-fsfacts`, item 26) cannot record a snapshot on its target clusters.
- fc0ac29 changed the design to make `ino` a decimal string. That contradicts
  decision 10, the code, and the frozen valid case, which hashes an integer `ino`.

I am not reopening decision 10. The point is that decision 10 and 11.1's
"everything else is a string" were always in tension. The agent's `E1001` pin
turned that tension into a failure. It belongs in the agent's "decided without
guidance" list as a collision, not a pin.

**Fix (owner ruling needed).** Either amend decision 10 to make `ino` a decimal
string, record it in `docs/decisions.md`, and change `events/mod.rs`, the
valid and invalid `file.version_added` cases, and their IDs. Or keep the integer
and state in 11.16 how an inode above 2^53 is recorded. The first is the only
option that works on the target filesystems.

---

## Majors

### M1. A newer op, steps, assertion, or spec version inside a body makes the event invalid instead of opaque

**Where:** 11.8 validation-codes hunk ("an unknown `op` or op version" is
`E6002`); 11.16 hunk (a value not of the form 11.8, 11.9, or 11.7 states is
`E1002`); `steps.rs:250` and `:413`.

**Collides with:** 11.11 ("an unknown event type or a higher version is read as
opaque and never rewritten"; the list of versioned formats includes "steps, each
op, assertions, parse spec") and 11.8 ("a change is a new version number, and
the old one stays runnable").

**Finding [Certain].** Each op is versioned separately so that `filter` v2 can be
added without changing `claim.added`. Under these rules, a gate A binary reading
a later chain that has a `claim.added` (tv 1) with `{"op":"filter","v":2}` rejects
the whole line as `E1002`. That is the code the Record treats as corruption at
the line level. To avoid this, every later op version would also need a tv bump
on `claim.added`, `claim.revised`, and `file.parse_spec_set`, which makes the
per-op versioning pointless.

**Fix.** In 11.16, state that inside a body, a nested format version above the
reader's highest known version (`steps`, any op `v`, `assert`, `spec`) makes the
event opaque, the same as an unknown `tv`. `E6002` for an unknown version stays
for author input at `claim.add` and `claim.propose`. Add one envelope case:
`claim.added` with `filter` v2 is read as opaque.

### M2. Gate A bodies reject session refs and output files, and two frozen invalid cases lock that in

**Where:** 11.16 handle hunk ("Handles are `F<n>`, `F<n>@v<m>`, `C<n>`, and
`R<n>` … `S<n>.<m>` refs are gate B"); `envelope/claim.retracted.invalid.json`
(`"ref":"S4.12"` → `E1002 /body/ref`); `envelope/file.registered.invalid.json`
(`"kind":"output"` → `E1002`).

**Collides with:**
- Glossary "Ref … Never changes across revisions".
- 7.1, and CUJ 4 and CUJ 5(a): promoted `S4.12` gets main-chain
  `claim.revised`, `claim.reviewed`, and `claim.retracted` events with ref `S4.12`.
- 5.4 supersede: `supersedes` may be `S4.3`.
- 11.22: the span ref grammar is `C[0-9]+|S[0-9]+\.[0-9]+`, which also allows
  `C007`. 11.16 refuses `C007`, so a span can name a ref no claim can have.
- 6.1 Kind `output`, which gate B (`propose_title` for an output) and gate C register.

**Finding [Certain].** Gate B must either bump tv on six `claim.*` types and on
`file.registered`, or edit frozen cases. Neither cost is recorded.

**Fix.** Accept `S<n>.<m>` syntactically in the tv 1 claim-ref form now. The
grammar is already fixed by the glossary and 11.22, and refusing an `S` ref is
an operation-level check, not a format check. Align 11.22's regex with the
no-leading-zero rule. Drop the two invalid cases, or move them to operation-level
tests. If the owner prefers the tv bump, record it in `docs/decisions.md` as a
gate B and C cost.

### M3. `src` is mandatory, closed, and the same for every actor, so D27 has nowhere to record its confirming event

**Where:** 11.2 validation hunk ("`src` is exactly `{"source","tty","harness_env"}`",
on every envelope); `envelope.rs:316-349`.

**Collides with:**
- 11.2's own text: `src` "carries the D23 facts **for owner actions**".
- 3.2 and D27: a viewer-confirmed action becomes a main-chain event "with
  `source: viewer` **and the hash of the viewer event that confirmed it**".

**Finding [Certain].**
- With envelope keys closed at `env` 1, `src` closed, and body keys exact per
  `tv`, that hash fits nowhere without `env` 2 or a tv bump on every viewer-actionable type.
- Session-actor journal lines (gate B) must carry a `source` from
  {`cli`, `static-view`, `viewer`, `wrapper`}, and none of them describes a tool server.

**Fix.** Make `src` required only when `actor.kind` is `owner`, and absent or
`null` otherwise. Add a reserved `src.confirmed_by` (an event ID) to 11.13 now,
so gate D needs no version bump.

### M4. "Any JSON for values whose form 11.16 never states" covers fields whose form is stated

**Where:** 11.16 hunk ("a value whose form is stated nowhere is accepted as any
JSON value in gate A"); `events/files.rs` and `events/claims.rs` (`Form::Any`);
`docs/decisions.md` entry "event bodies are schema-checked JSON".

Field by field [Certain]:

| Field | Form is open? | Where the form is stated |
| --- | --- | --- |
| `file.version_added.content_label` | **No** | 11.16 row: "one of 6.2's three values or `null` for a first version". The code has `Form::Any`. |
| `file.registered.title`, `.description` (and `claim.*.sentence`) | **No** in substance | Section 9 `file.register` ("title and description present", `E3002`); 6.1 Level (registered means title and description); G8. `Any` accepts `null`, which produces a "registered" file with no title. 11.22 drift normalization needs `sentence` to be a string. |
| `file.version_added.size`, `file.kept.compressed_size` | **Mostly no** | 11.1 (integers within ±2^53 are JSON integers); 11.17 `file_versions.size INTEGER`; decision 10's `snapshot.size` is an integer. Only a directory version's `size` is genuinely open. |
| `claim.checked.assertion.observed` (and `.expected`, `key_map`) | **Partly** | The 11.16 preamble says table-derived values use 11.6 typed values. A cell is table-derived, `expected` is the 11.9 shape, and `key_map` is a list of key tuples of typed values. Only `observed` for an absent row, and for `rank`, `count`, and `exists`, is open (readiness B3 lists "observed when absent"). |

The general problem is that `Any` in a hashed body lets the same fact be encoded
two ways. For example, `"size":1048576` and `"size":"1048576"` both validate and
give two IDs. Tightening later at the same `tv` would make frozen-valid lines
invalid, which 11.11 does not allow.

**Fix.** In 11.16, list a form for every key. Use `Any` only for the two
genuinely open cases (absent-row `observed`, directory `size`), and say each is
decided before P4 or P3. Pin the spelling of `content_label`: 6.2's values
contain spaces, while `origin.kind` and the outcome values use snake_case
(`changed_outside_run`, `not_checkable`). Choose one and freeze it in the valid
case, which currently hashes `"content changed"`.

### M5. Assertion evaluation outcomes are frozen in P1, in the same directory P5's validator must reject

**Where:** 11.9 and 11.18.3 hunks; `conformance/formats/assertions/` has 28 of
51 cases carrying `observed`: 25 expect an outcome and 3 expect an
evaluation-time `E6004` (`date_vs_datetime`, `str_vs_number`,
`compare_rows_str_tol`) [Certain].

**Collides with:**
- The P5 checkpoint, `validate.py conformance/formats/steps/invalid
  conformance/formats/assertions` "rejects every invalid case". A schema validator
  cannot produce `E6004 /expected` without evaluating against `observed`, so P5
  either fails or `validate.py` grows an 11.18.3 evaluator.
- D21 and item 87: semantics that the P4 third session and owner sign-off of
  `checker.md` (which contains 11.18) are meant to judge independently are now
  authored and frozen by the Rust implementer.
- 7.3: after add, outcomes are recorded, not refused, and `error` is "reserved for
  failures of the checker itself". An evaluation-time `E6004` during a re-check
  (gate C: a new version whose column type changed) maps to no verdict.

**Fix.**
- Split `assertions/` into `assertions/schema/` (P1, validator scope) and
  `assertions/eval/`.
- Move `eval/` into P4's `conformance/checker` set, or mark it provisional until
  `checker.md` sign-off.
- Type-check the assertion against the final step's schema at validation time.
  `TableSchema` already exists, so the mismatch is caught before anything runs.
- State that a type mismatch found at re-check is verdict `error` with `E6004`.

### M6. The negative-zero rule from decision 3 was changed without a record

**Where:** 11.6 `dec` hunk: "Negative zero is written `0`" becomes "written
without its sign, scale kept (`-0.00` is `0.00`)".

**Collides with:** `decisions-p1.md` decision 3 ("negative zero is written `0`"),
which is settled.

**Finding [Certain].** The agent's reading (strip the sign, keep the General
Decimal Arithmetic scale) is defensible and probably what was meant. Even so, it
changes the literal text of a do-not-reopen decision, it changes
`final_rows_hash` for every pipeline that yields `-0.00`, and there is no entry
in `docs/decisions.md`.

**Fix.** The owner confirms the reading, and `docs/decisions.md` gets a
"clarification of decision 3" entry. If the owner meant literal `0`, revert and
regenerate the affected typed-value cases.

### M7. The report handle format is deferred to P6, but frozen cases already hash one

**Where:** 11.16 hunk ("The format of a report handle is not fixed in gate A (it
is decided before P6), so a `report` value is any non-empty string");
`envelope/claim.added.valid.json` (`"report":"paper.md"`).

**Finding [Certain].** Any P6 choice must keep `paper.md` valid or edit a frozen
case. "Any non-empty string" also flows into 11.20's `reports/<report>.html`
file name, which allows `/` and `..`.

**Fix.** Decide the report handle grammar now, in 11.16. It is a hashed format
that P1 already freezes. Make the valid cases use a value that fits the chosen
grammar.

### M8. The P1 checkpoint claim overstates what the Python side checks

**Where:** 742e0ae commit message ("the two write identical file and subject
IDs for all 254 cases"); Verification Plan, Conformance: formats row ("every
`steps/invalid/` case is rejected with the named code and field **by both**").

**Finding [Certain].** `reference/formats/test_formats.py` checks the meaning of
only `canonical/` (17), `ids/` (7), and the line-level part of `envelope/` (60).
For the other 170 cases, the "subject ID" is the JCS hash of the input, and the
"file ID" is the JCS hash of the case file. Equal hashes there show the two
canonicalizers agree. They do not show agreement on any validation result. That
is consistent with the phase table, since `validate.py` is P5, but the row says
"by both" at P1, and the agent did not flag the conflict.

**Fix.** Amend the Conformance: formats row to say Python validation of `steps/`
and `assertions/` lands at P5 (item 87's `validate.py`). Reword the checkpoint
claim in the next report.

---

## Nits

- **N1. 11.1 hunk.** It cites I-JSON (RFC 7493) but accepts ±2^53 inclusive
  (`canonical.rs:478`). RFC 7493's range is ±(2^53−1). Either say "within ±2^53,
  inclusive, which is wider than I-JSON by one value at each end", or use 2^53−1.
- **N2. 11.1 and d6dd8c4.** The fault-precedence rule is pinned to how Python's
  `json` module happens to behave. `field` is `null` for `E1001` and `E1003`
  anyway, so this matters only when both codes apply to one input. Consider
  "either code is conforming" rather than freezing an implementation artifact.
- **N3. 11.6 lexical hunk.** "`str` `v` must already be NFC" refuses non-NFC
  input. 11.6 says "NFC-normalized", which suggests normalizing. Hand-written
  assertion files from macOS editors will hit `E1004`. State which is intended.
- **N4. Section 9 hunk.** `claim.add` and `table.spec` gained `E3105` and
  `E6004`–`E6007`, but not `E1001`, `E1003`, or `E1004`. All three are reachable
  from `--steps`, `--assert`, and spec files.
- **N5. 11.8 hunk.** `E6002` now covers about ten distinct faults (unknown op,
  unknown key, an uncovered join key, a duplicate output column, and others).
  G2's measure ("rejects every case that omits a required choice") can no longer
  be read off the code. Readiness already lists "`E6002` has two producers".
  Consider giving schema faults their own code.
- **N6. Item 3 check.** `E6006` was proposed in runbook section 1 for the
  `claims.max_final_rows` refusal. The owner chose no cap, so there is no live
  collision, but note that the code is now taken.
- **N7. Spec drift row hunk.** "As `spec-drift.sh` states" makes the script
  normative. The design should state the rule and the script should follow it.
- **N8. Item 90 and item 4 check.** No conformance case produces `E3105`,
  `E3103`, or `E6007`, because there is no `parse_spec/` or `citations/`
  directory. The 11.7 and 11.23 CLI grammars are frozen only by unit tests and
  are not cross-checked. Readiness lists "no `citations/` conformance cases".
- **N9. decisions.md "Arrow conversion errors use E3102".** If P4 converts
  derived values through `to_arrow`, overflow surfaces as `E3102` instead of
  11.6's `E5002`. That is the boundary the agent rightly declined to decide.
  Note that `to_arrow` must not be on the derive path.
- **N10. 11.8 hunk.** "A step that would add a column whose name is already
  present is `E6002`", combined with 11.18.2's fixed `rank` output name, means a
  table with its own `rank` column cannot be ranked without a `select` first.
  This is acceptable, but should be stated.
- **N11. 11.2 hunk.** An `env` above 1 is opaque, yet its line form and `id` are
  still checked under env 1 rules. That rules out changing ID derivation in a
  later envelope version (`record.json` carries `hash_alg`).
- **N12. Scope.** 742e0ae edits `ci.yml` (item 6) and `deny.toml` (item 4), both
  P0 items. Both are logged in `docs/decisions.md` and justified, so no action
  is needed.

---

## 1. The twelve decisions

I took "twelve" to be the bullets under "Formats pinned that P1 freezes and the
design had not fixed" in 40f1f35, counting item 17, item 20, and section 9
separately. The 2026-11-11 entries in `docs/decisions.md` are covered where they
bear on a finding.

| # | Decision | Consistent? | Collides with | Later phase |
| --- | --- | --- | --- | --- |
| 1 | 11.1 I-JSON, `E1003`, floats including `-0` and >2^53 as `E1001`, fault order | Mostly | Decision 10 and 11.16 snapshot `ino` (B2); RFC 7493 range (N1) | **P2 blocked on inode64 filesystems** (B2) |
| 2 | 11.2 genesis and envelope validation | Genesis yes; `src` no | 11.2 "for owner actions", 3.2 and D27 (M3) | Gate B session lines, gate D viewer actions (M3); env 2 (N11) |
| 3 | 11.6 lexical forms, `E1004`, negative zero, datetime form | Mostly | Decision 3 (M6); 11.6 "NFC-normalized" (N3) | None beyond the hash change |
| 4 | 11.7 `type`, `scale`, nulls for parquet, `E3105` | Yes, with 11.6 and item 47 | None | `scale` is fixed at spec time, so a new version with more decimals is `E3102` until a new spec. 11.6 already says this; worth stating in gate C |
| 5 | 11.8 op form `v`, validation codes, `E6005`, `E6006` | No | 11.11 and 11.8 per-op versioning (M1); G2 readability (N5) | **Every later op version** (M1) |
| 6 | 11.12 `field` null; nested errors as `E1002` | Yes | None | None |
| 7 | 11.16 handles, exact keys, report handle, claim `kind` | No | Glossary Ref, 7.1, 11.22, 6.1 Kind (M2); 6.2, 11.1, 11.17, 11.16 preamble (M4); 11.20 path (M7) | **Gate B and C tv bumps** (M2); P6 report handle (M7) |
| 8 | 11.18.1 reserved words, call vs column, syntax errors | Yes, with 11.18.4 | None | None |
| 9 | 11.23 CLI parsing, `E6007` | Yes | None (`--cited-keys` still undefined, which runbook section 6 defers to P8) | None |
| 10 | Item 17 gains the `{expr}` serializer | Yes (decision 7 assigns it to item 17) | Freezes 11.18.4 rendering before P4 sign-off (part of M5) | P4 sign-off |
| 11 | Item 20 lists four formats codes | Yes | None | None |
| 12 | Section 9 rows list the new codes | Incomplete | N4 | None |

## 2. Body values accepted as any JSON

See M4. `content_label` is fully specified, as the brief said. Titles and sizes
are effectively specified by section 9, 6.1, G8, 11.1, and 11.17. `observed` is
partly specified by the 11.16 typed-value preamble.

## 3. The six new codes

| Code | Fits 11.12 numbering | Used elsewhere in the design | Gate matches its operation |
| --- | --- | --- | --- |
| `E1003` | Yes (formats) | No | A. Any JSON input |
| `E1004` | Yes (formats) | No | A. Typed values in steps and assertion files; reported as `E1002` inside envelopes |
| `E3105` | Yes (files, `E31xx` is the `table.spec` block) | No | A. `table.spec` |
| `E6005` | By precedent (`E6002` and `E6004` are already raised by `tm-formats`); strictly, 11.12 numbers by library | No | A. `claim.add`, `claim.propose`, `preview_steps` and `describe_steps` |
| `E6006` | Same as `E6005` | Not in the design; runbook section 1 proposed it for a cap the owner declined (N6) | A |
| `E6007` | Same as `E6005`. It is a CLI-argument fault, so `E8xxx` would also fit, but item 19 puts the parser in `tm-formats` | No | A. `tm claim add` only; the JSON form uses `E6002` and `E6004` |

All six are absent from the design at HEAD (B1).

## 4. Are all 254 case files exercised?

**[Certain], from reading `crates/tm-formats/tests/conformance.rs`; not run.**
`conformance_formats` walks `conformance/formats/` recursively and collects every
`*.json` file. There are 254 such files and no other files under the tree. It
dispatches each file on its top-level directory and fails on an unknown one. It
asserts that each of the seven directories is non-empty. `steps/valid` and
`steps/invalid` both dispatch as `steps`. No file is orphaned.

The "3 tests" are this one walker plus two structural checks: every
`steps/invalid` case names a code and field, and every gate A type has a valid
and an invalid line, with all seven citation kinds present.

On the Python side, see M8: only 84 of the 254 are checked for meaning.

## 5. Readiness findings the agent says it resolved

Status is given at 742e0ae. **At HEAD, every one of these is open again (B1).**

| Finding | Status at 742e0ae | Why |
| --- | --- | --- |
| B3 (P1 part) | **Partly closed** | Closed: the assertion `row` and `compare_rows` shapes, `tol` with ordering operators, reversed `between` bounds, return types, aggregate types, join key inclusion, and duplicate column names. Still open and in P1's item 18: `observed` for an absent row (claim.checked shape; M4). The P1/P4 split of the 17 facts is not written down anywhere, and the report still lists B3 as one block. |
| style-1 | **Closed** | 11.16 now says "not declared" and "read as opaque", matching item 14. |
| style-11 | **Touched** | `between` per end and the `compare_rows` comparison are closed. `ties: max` ("inverse RANK") is untouched and the 11.18.2 `rank` row still says it. The agent's own "ties-independent parts" admits this. |
| M8 (last part) | **Closed, with a caveat** | Heading normalization is defined, but by reference to the script (N7). |
| plan-M8 (P1 part) | **Touched** | `cells.columns` now has a decided CLI behaviour (none, so `columns: null`). Item 83 still says `--cited HANDLE...` rather than `CITATION...`. Section 12's verbs and `--cited-keys` are correctly left for P8 (runbook section 6). |

## 6. Scope

- **Built outside items 11–25, 88, 90, 93, 94:** only the `ci.yml` and `deny.toml`
  edits (items 6 and 4), both logged (N12). The `{expr}` serializer, the
  11.18.2 type inference in `expr.rs`, and the Arrow mapping are all inside
  items 15 and 17 as written.
- **Fixed something runbook section 6 defers:** nothing. `--cited-keys`,
  section 12's verbs, the backtick rule for the other 11.18.4 placeholders, and
  `action_file.rs` are all untouched. Not building decimal arithmetic or `E5002`
  (logged) was the right call.
- **Decided ahead of its phase without saying so:** the 11.18.3 evaluation
  outcomes and the 11.18.4 `{expr}` rendering are P4 semantics frozen in P1 cases
  (M5).
