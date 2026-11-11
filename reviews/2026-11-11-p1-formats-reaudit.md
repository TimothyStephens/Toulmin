# Re-audit: PR #1 (p1-formats) at 4352d5f, against the 2026-11-11 review

Reviewer session, 2026-11-11. One pass, scoped to whether each finding is
closed. **[Certain]** means I read it or ran a read-only check this session;
**[likely]** means a strong inference. The test suite was not run, and there is
no `cargo` in this sandbox, so nothing was built.

## Verdict

**Merge.** Every Blocker and Major is closed. One hygiene nit from the rework
hunks (two committed `.pyc` files) does not block.

## Three specific checks

| Check | Result |
| --- | --- |
| Every design edit precedes the case that freezes it | **Holds [Certain].** The rework is three design/code pairs, each design commit first: 0ec3304 → 837fef9, 2c41f45 → 2fcc2ec, 928b33a → 4352d5f. The three design commits touch only `docs/`; the three code commits touch no file under `docs/design/`. Spot checks: `ino` as a string, `T<n>`, the two nested-version opaque cases, `src: null` on the journal line, snake-case `content_label`, and the 2^53−1 `int_limits` case each appear in the code commit after the design commit that states them. 3d8887e restores the design byte-identically to 742e0ae (`git diff 742e0ae 3d8887e -- docs/design/toulmin_v1_design.md` is empty). |
| `scripts/ci/spec-drift.sh` passes at HEAD | **Formats half passes [Certain]:** `OK docs/spec/formats.md matches 11.1 … 11.23`. **Errors half not runnable here:** it needs `build.rs` output and this sandbox has no `cargo`. By hand, the 74 codes and gates in `crates/tm-formats/src/catalog.rs` equal the 74 rows of `docs/errors.md` [Certain], so the byte comparison passes on CI [likely]. Unverified: the generated file's exact bytes. |
| `conformance/checker/assertions/eval/` carries no `expected.json` | **Holds [Certain].** 28 directories, each with exactly `case.json` and `observed.p1.json`. No file named `expected.json` anywhere under `conformance/`, and no `"expect"` member in any `case.json`. |

## B2 and M1 to M8

| Finding | Status | Why |
| --- | --- | --- |
| B2 `ino` | **Closed** | 11.16 makes `ino` a decimal string, with the reason stated; `events/mod.rs` checks `Form::IntStr`; the valid case carries the string; a dated note sits under decision 10 in `decisions-p1.md` and an entry in `docs/decisions.md`. |
| M1 nested versions | **Closed** | 11.16 says a `steps.steps`, op `v`, `assert`, or `spec` above the known one makes the event opaque, with `E6002` kept for author input; `envelope.rs` checks `nested_version_above` before the body schema; two opaque cases (`claim.added.filter_v2`, `file.parse_spec_set.spec_v2`). |
| M2 refs and `kind` | **Closed** | `ClaimRef` parses `C<n>` and `S<n>.<m>`; 11.16 says the ref form is the same in every gate and acceptance is the operation's check; 11.22's regex now has no leading zeros; `kind` is `input` or `output`; the two freezing invalid cases are gone and valid cases carry `S4.12` and `output`. |
| M3 `src` | **Closed differently, acceptably** | I proposed "absent or null" for non-owner actors; the agent chose `null` only, so one fact has one encoding. Better than my proposal. `src.confirmed_by` is reserved in 11.13, parsed as an event ID, and covered by two cases. |
| M4 body forms | **Closed** | 11.16 states a form for every key; the single `Form::Any` left is `claim.checked.assertion.observed`, named as item 18's P4 decision. `content_label` is snake_case with an invalid case for the spaced form; titles and descriptions are non-empty strings; sizes are integers. |
| M5 eval cases and typing | **Closed differently, acceptably** | Eval cases moved to `conformance/checker/assertions/eval/` as the owner laid out: `case.json` plus `observed.p1.json`, with P4's `expected.json` to come from `checker.md` and `case.json` only. The P5 checkpoint now names `assertions/schema`. 11.9 states the validation-time check against the final table's schema and the verdict `error` with `E6004` at re-check; `Assertion::check(&TableSchema)` exists with twelve schema cases carrying a `table`. |
| M6 negative zero | **Closed** | Owner confirmed the reading; clarification of decision 3 logged in `docs/decisions.md`; no design text changed. |
| M7 report handle | **Closed (ruling changed to `T<n>`)** | 11.16, section 9's `report.add` row, 11.20's `reports/T<n>.html`, `ReportHandle`, and the valid cases all carry `T<n>`. No `R<n>`-as-report leftover in the design [Certain]. The `R<n>` collision and its re-ruling are both recorded in `docs/decisions.md`. |
| M8 Python coverage | **Closed** | The Conformance: formats row says Python checks `canonical/`, `ids/`, and the line level of `envelope/` at P1, and `steps/` and `assertions/schema/` from P5; the rest is a hash agreement at P1. |

## Declined nits

| Nit | Reason given | Holds? |
| --- | --- | --- |
| N2 fault precedence | "Either code is conforming" would make `float_before_duplicate` and the Python check ambiguous; the rule is in 11.1, not left to an implementation | **Holds.** A conformance case needs one expected code; the alternative is to drop the case, not to loosen it. The rule is now design text, which answers my objection. |
| N5 separate schema-fault code | A new code the catalog gates and section 9 would carry, for a readability gain G2 does not require; `field` already locates each fault | **Holds.** G2's measure is about rejection, not code identity. |
| N6 `E6006` | No change needed; noted that the code is taken | **Holds.** The note was all I asked for. |
| N11 env > 1 still checks the ID | ID derivation changes through `record.json`'s `hash_alg`, not the envelope version; a line's ID must be checkable to chain `prev` | **Holds.** The `prev` argument is decisive: a reader that cannot verify an ID cannot verify the chain, so an opaque envelope still needs a checkable ID under the record's declared algorithm. |

## Other observations from the rework hunks

- **Nit [Certain]:** 837fef9 committed `reference/formats/__pycache__/canonical_json.cpython-311.pyc` and `ids.cpython-311.pyc`; both are still tracked at HEAD and `.gitignore` has no `__pycache__/` entry. Harmless to the design, but the reference implementation is meant to be read with `-I` from source, and a stale `.pyc` beside the source is the kind of thing a later Python session trips on. Remove them and add `__pycache__/` to `.gitignore`.
- Harness coverage [Certain, from reading]: `tests/conformance.rs` gained `parse_spec` and `citations` arms and asserts all nine directories are non-empty; the Python walker hashes every file (306). The valid/invalid pair exists for all 21 gate A types.
