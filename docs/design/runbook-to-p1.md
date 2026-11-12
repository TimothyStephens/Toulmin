# Everything outstanding, in order

Date: 2026-11-10. Written against `toulmin_v1_design.md` at 1,604 lines (the file sent after the twelve decisions). Sections 0, 6, and 7 are things only you can do; sections 1 to 5 are one patch I write once you answer; section 8 starts the build.

---

## 0. Merge the design fork

The repository copy is behind by three sets of changes it has never seen: the agentbox reconciliation, the P0 outcomes, and the twelve decisions. Patches will not apply cleanly, so this is a replace-and-verify.

```bash
cd ~/repos/toulmin
git checkout main && git pull
git ls-files | grep -i toulmin_v1_design     # confirm the path, call it $D
cp "$D" /tmp/repo-design-before.md
diff -u /tmp/repo-design-before.md ~/Downloads/toulmin_v1_design.md > /tmp/fork.diff
less /tmp/fork.diff
```

Read every `-` line in `/tmp/fork.diff` that is not immediately followed by a `+` line saying the same thing differently. Your builder's three edits from 1b2ec63 (the D24 pointer, item 1's duckdb features, item 5's toolchain note) will appear that way, because I wrote equivalents independently. If a `-` line carries anything I do not have, stop and send me the hunk. Otherwise:

```bash
cp ~/Downloads/toulmin_v1_design.md "$D"
git add "$D" && git commit -m "design: agentbox reconciliation, P0 outcomes, twelve P1 decisions

Supersedes the three design edits in 1b2ec63 with equivalent wording.
Adds the agentbox interface reconciliation (2026-11-10), the P0
portability and determinism outcomes with the glibc 2.17 libm
measurement, and the twelve decisions in decisions-p1.md.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01WRpgBkRRnParJ8VhF4vNq1"
git rev-parse --short HEAD
```

Also commit, on the same branch or a docs branch: `decisions-p1.md` under `docs/design/`, and the glibc 2.17 libm result appended to `docs/decisions/musl-spike.md` under a "glibc 2.17 libm check" heading, with the note that nothing builds or runs the fallback until the CI job switches on at P8.

Send me the short hash. Everything after this works from the repo.

---

## 1. Where `claim_keys` comes from, and what `E6003` means

**The hole.** Section 1.1 says a quantitative claim's grounds are the cells its steps produce, mapped back to input keys. `claims.citing_rows` reads `claim_keys(ref, version, key_tuple)`. Nothing populates that table. `claim.checked` stores `key_map_hash`, a digest, not the tuples. Separately, section 9 makes an absent cited key a refusal of `claim.add` (`E6003`) while 7.3 records it as verdict `fails`; an implementer must pick one.

**Recommendation, four parts.**

- `claim.checked` gains `key_map`: the list of key tuples in final-row order, the same list `key_map_hash` digests. The Claims fold populates `claim_keys` from the newest verdict for each ref and version. The author's citations are stored exactly as given and are never rewritten, because a citation is what the author claimed and the key map is what the check found; they diverge legitimately when a new version arrives.
- A new setting `claims.max_final_rows`, default 10,000, gate A. A claim whose final rows exceed it is refused at add time with `E6006`. Rationale: the key map is stored in a hashed event and shown on the claim page; a claim nobody can review is not evidence, and without a cap one claim can put a megabyte in the chain.
- `E6003` is narrowed: it fires only when a `cells` citation names a key absent from the pinned version **at add time**, which is a typo in the command. 7.3 governs everything after that, including re-checks against a new version where the key has gone: verdict `fails`, not a refusal.
- `steps` may be `null` on a claim that carries an assertion. The final table is then the cited rows named by the `cells` citation, in key order. This is what the fixture's claim 10 needs and it makes a single-cell `value` assertion work without a pipeline.

**Changes.** 11.16 `claim.checked`; 11.17 fold list and item 70; section 9 `claim.add` row; 7.3's note; 11.19 new setting; item 20 new code; 11.8's `steps` optionality; item 65.

**Answer needed only on the cap.** 10,000 with a refusal, a different number, or no cap at all.

---

## 2. How a span reaches the `spans` table

**The hole.** `spans` is fold-written from `report.registered.spans`, but a report is registered before its claims exist, so a span added afterwards never reaches the table. `reports.spans`, the drift check, and the two span flags then have no source, and the system runs without ever flagging anything.

**Recommendation.** Make spans scan-derived rather than event-derived, exactly like the stat cache. A registered report's spans are disk state in a git working tree that Toulmin does not control, so trying to track every edit through the chain is the wrong shape.

- `report.registered` carries `{"report","path"}` only; the `spans` key goes.
- `tm report check` and `tm status` rescan every registered report from the working tree and rewrite the `spans` table, which moves out of the fold's ownership and joins `stat_cache` as a table excluded from rebuild drops.
- `points at a retracted claim` and `points at a superseded claim` move from the fold-written list to the scan-derived list in item 66, because they depend on a scanned span and a chain state together.
- Drift compares `claim_revisions.sentence` with the scanned span text, normalized per 11.22.

**Changes.** 11.16 `report.registered`; 11.17 `spans` ownership and the drop rule; item 66; item 70 (drops `spans`); item 74 `report_check.rs` and `status.rs`; item 78 `reports.rs`; CUJ 6's wording.

**No answer needed** unless you want the event-based alternative, which costs a new event type written by three operations.

---

## 3. Who writes `expected.json`

**The hole.** Item 59 says the Rust example binary writes `expected.json`; item 87 says the Rust Checker never writes it. If the Rust side produces the expected values, the Python checker is being compared against Rust rather than against the spec, and D21 buys nothing.

**Recommendation.**

- `expected.json` for all 60 conformance cases is written by a third agent session that reads `docs/spec/checker.md` and the case inputs only, and is signed off by you at the existing P4 checkpoint. Neither implementation is consulted or exists yet when it runs.
- Item 59's example binary writes `observed.json`, never `expected.json`. Same for `refcheck.py`.
- A difference between the two implementations is recorded in `docs/decisions/checker-diffs.md` and ruled against `checker.md`; whichever implementation disagrees with the spec is fixed, and the spec is amended only through that record.

That makes three independent readings of the spec: the cases, the Rust, the Python. It is the only arrangement in which a conformance failure tells you something.

**Changes.** Items 59, 87, 92; the P4 and P5 checkpoints; `docs/decisions/checker-diffs.md` and `checker-spec-review.md` added to item 93's file list.

**Answer needed.** Whether the cases come from a third session or from you directly. A third session is more independent; you are faster and already know the answers.

---

## 4. What the "gate A code list" is

**The hole.** The Verification Plan's error-codes test diffs observed codes against "the gate A code list". Item 20 defines the catalog as every code in sections 9 and 11, which includes gate B, C, and D codes that gate A can never produce, so the test fails forever or cannot be written.

**Recommendation.** Each catalog entry carries a `gate` field (`A`, `B`, `C`, `D`) in `tm-formats/src/errors.rs`. The error-codes test filters to `gate == A`. `build.rs` prints the gate as a column in `docs/errors.md`, so the catalog is self-describing and a code moving gates is one edit in one place.

**Changes.** Item 20; item 25; 11.12; the Verification Plan's Error codes row.

**No answer needed.**

---

## 5. Where the stat cache lives

**The hole.** 3.4 says "a node-local file beside the index"; 11.17 says a table inside the index database. Item 77 follows 11.17. Two implementers, two layouts, and `--since LAST` silently reads the wrong one.

**Recommendation.** The table inside the index database, as 11.17 has it: one file to locate, one integrity check, one thing `tm doctor` tests for locality, and the "excluded from rebuild drops" rule is already written. Fix 3.4's wording to match.

**Changes.** 3.4 one sentence.

**No answer needed.**

---

## 6. Let the build find these

Recorded so they are not lost, with where each surfaces. Do not fix them now.

| Finding | Surfaces at | Fix when it does |
| --- | --- | --- |
| `tm-record/src/scan.rs` resolves by event ID "via the index", inverting the crate graph | P2, `cargo build -p tm-record` | The Core looks up `(segment, offset)` in `events` and calls a Record read-by-position; the Record owns only the linear fallback |
| `view_export.rs` is in item 74 (P7) but needs item 79's templates (P8) | P7, `cargo test -p tm-core` | Move `view_export.rs` to P8, or stub it in P7 |
| Item 5a (the 8 MiB stack rule) is a rule about a P8 file placed in P0 | Reading the phase table | Move the sentence into item 83 |
| `bench-index.sh` has no entry point in P7 | P7 checkpoint | Point it at `examples/gen-chain.rs` plus a small runner |
| `verdict_data_changed` is in `conformance/record/tamper/` but `tm-record` cannot detect it | P2 checkpoint | Move that case to item 82's `audit_tampering.rs` only |
| `claim_events` is claimed by both the Core (11.17) and the Claims fold (item 70) | P6 or P7, duplicate rows | Give it to the Claims fold; remove it from the Core's list in 11.17 |
| Section 12 lacks `table describe` and `table preview --steps`, which item 83 has | P8, the G9 parity test | Add both verbs to section 12 |
| `--cited-keys` is in 11.23 but not item 83, and its file format is undefined | P8, writing `claim add` | Define it or drop it; the repeated `#col=value` form covers the common case |
| Item 66 calls several labels "flags" and stores them in `flags` | P6, writing the fold | Split the list by kind as 5.6 has it |
| 11.18.4's `{col}`, `{as}`, `{left_col}`, `{right_col}` have no backtick rule | P4, writing `describe.rs` | Same rule as `{expr}`: as written, backticks only where the grammar requires |
| The gate A half of `action_file.rs` has no `tm-formats` item | P8, writing `apply.rs` | Add it to item 20's neighbourhood |
| `tm-data` block layout cannot hold a query called twice on one page | P8, the parity test | A list of `{query, params, result}` |
| Item 83's `claim add --report PATH` takes a path; bodies carry a `T<n>` report handle (P1 ruling, 2026-11-11) | P7, writing `claim_add.rs` | `report add` allocates `T<n>`; `claim add --report` accepts a path or a handle and resolves a path through `reports` |
| Verification Plan's Scenario row says eight tests; item 82 lists ten | P8, writing `tests/scenarios.rs` | Make the row say ten |
| Fuzz job is wired to `pull_request`; the Verification Plan says scheduled | P4, when `tm-checker` gives it targets | Move it to a schedule, or it adds ten minutes to every PR from P4 on |
| `conformance/checker/assertions/eval/<name>/observed.p1.json` holds outcomes the Rust side chose in P1 | P4, before the third session starts | That session writes `expected.json` from `checker.md` and `case.json` only and must not open `observed.p1.json`; put that line in its prompt |
| 11.10's `COLLATE BINARY` spelling was rejected by DuckDB's parser in a P2 test | P4, writing the translator | State the spelling DuckDB accepts, or state that no `COLLATE` clause is the binary default and 11.10 means that |

---

## 6a. After P1 merges

`.local-tools/` in the agentbox checkout holds about 2 GB of P0 toolchain material the builder said could go once P1 landed. `du -sh .local-tools && rm -rf .local-tools` there, not in the Toulmin repo.

## 7. agentbox artifacts, on a host with podman and OMP

Off the critical path; gates drafting the gate B design document, which is after P8.

```bash
cd <agentbox repo> && git checkout toulmin-gate-b
make itest
python3 -m unittest tests.integration.test_toulmin     # artifact 3 and the inspect grep
make omp-env-probe                                      # deviation 6
```

If the inspect grep finds the token, nothing else on that branch matters until it is fixed. Ask the builder one question whatever the result: confirm that `mount_socket_dir` is exempted from the `must_not_mount` glob check and that the exemption covers only that one directory.

---

## 8. Start P1

After sections 0 to 5 are committed. P1 is `tm-formats`, its conformance cases, and the Python formats cross-check; its checkpoint is `cargo test -p tm-formats` and `python3 reference/formats/test_formats.py` passing with identical hashes for every case.

Prompt for the implementing agent:

```
Task: P1 of the Toulmin V1 gate A plan. Read docs/design/toulmin_v1_design.md,
specifically sections 11.1 to 11.23, the Detailed Implementation Plan's phase
table and items 11 to 25, 88, 90, 93, and 94, and the Verification Plan rows
for Conformance: formats and Spec drift. Build exactly those items and nothing
else.

Before writing any file under conformance/formats/, re-read
docs/design/implementation_readiness_report.md, session 2 round 1. Every
Blocker and Major there that is not already resolved in the design must be
resolved by editing the design document and recording the decision in the
commit message, never by deciding it in code first. A conformance case freezes
a format; the design is where a format is decided.

Phase checkpoint: cargo test -p tm-formats passes, and
python3 reference/formats/test_formats.py passes, and the two produce identical
IDs for every case under conformance/formats/. Do not start P2.

Report: the checkpoint output, every design edit you made with its reason, and
any item you could not build without inventing something, as a numbered list.
```
