# Stage 9: Implementation Readiness Test (round 1)

*   **Target:** `toulmin_v1_design.md`, DetailedRange lines 1149-1320 (Detailed Implementation Plan, build gate A)
*   **Reviewers:** Implementation Readiness and Codebase Details (implementation-readiness-standard), Style (persona vectors plus the author's conventions). All three completed in isolated contexts. Codebase Details ran in plan-consistency mode: there is no code, so it audited the plan against the architecture (lines 1-1148) and the four inputs.
*   **Date:** 2026-11-09

Finding IDs: `impl-1-n`, `plan-1-n`, `style-1-n` (Stage 9 numbering, distinct from Stage 7's). Duplicates merged under the first ID.

**Totals:** 5 Blockers, 22 Majors (after merging), 16 Nits.

**Verdict:** the plan does not meet the Zero-Decision Rule or the Phasing requirement. The architecture carries most format decisions, but five things the plan depends on exist nowhere in the document (event body schemas, index schema, Checker semantics and `derive` grammar, static-view page set, settings table), and the plan has no phases. The Stage 8 gate A scope problem (B/C/D items in gate A, `tm apply` and `tm upgrade` contradicting the gate table, binaries that do not exist in gate A) was re-found by all three reviewers as a readiness blocker, not only a scope one.

---

## Blocker

### impl-1-1: no phasing, build order, or checkpoints
**Scope:** whole plan (lines 1155-1289); standard "Phasing & Checkpoints". Duplicate: plan-1-19.
A flat list of 97 files by crate. Load-bearing orderings contradict the listing order: the musl spike (item 9) must precede item 1, which already pins `mimalloc`; conformance runners (items 24, 41, 59) depend on the cases (items 90-92) listed last; item 87 (Python checker) must follow items 92 and 93; item 93 must precede item 87 and arguably item 53. **Guidance:** a phase table (P0 spike and decision record; P1 `tm-formats` + `conformance/formats` + `reference/formats`, checkpoint both test commands green with identical hashes; P2 `tm-fsfacts`, `tm-record`, `conformance/record`; P3 `tm-owner-cap`, `tm-files`, `tm-checker` + `conformance/checker` + `docs/spec/checker.md`; P4 Python checker by a separate session, differential suite; P5 `tm-claims`; P6 `tm-core` index, ops, read interface; P7 static view, audit, CLI, scenarios, smoke), each with the exact command and pass condition that closes it.

### impl-1-2: event body schemas do not exist
**Scope:** item 14, item 90 (`envelope/*.json`), item 75, item 93.
11.2 defines the envelope and 11.4 one body. No other body is defined: `project.created`, `settings.changed`, `file.registered`, `file.version_added`, `file.parse_spec_set`, `file.kept`, `file.renamed`, `file.panels_set`, `file.checksum_checked`, `file.hashing_finished`, `claim.added`, `claim.checked`, `claim.revised`, `claim.reviewed`, `claim.retracted`, `claim.superseded`, `report.registered`, `audit.completed`, `chain.torn_tail_truncated`, `lock.taken_over`. The gate A type set is never enumerated. `claim.checked` is the worst case: section 7 and CUJ 6 list what a verdict stores, with no field names or nesting. These are hashed, permanent, versioned records. **Guidance:** a per-type body schema for every gate A type in the 11.3 key style, and the gate A type set listed in item 14.

### impl-1-3: the index schema is named but not specified
**Scope:** item 76, items 51 and 71, item 77, rule 6.
Only table names. No columns, keys, indexes. Missing tables the design requires: the head record (main-chain head plus per-journal last line ID), settings (for `settings.show` "with the event that set each"), opaque-events-skipped (11.11), event-ID-to-position (section 9 says owner operations resolve targets by event ID; `scan.rs` offers only by-type and after-position scans), and whatever backs `files.list` text search and the computed cited count. `IndexTx` has no API. **Guidance:** `schema.sql` in the plan or an appendix; define `IndexTx`; state the fold contract (idempotent per event, transaction per batch).

### impl-1-4: `docs/spec/checker.md` cannot be written from the design because the Checker's semantics are not in it
**Scope:** item 93, item 87, item 17, items 53, 55, 56, 92. Duplicates: style-1-9, plan-1-16 (section 8 is staleness, not Checker semantics).
Undefined: the `derive` and `filter where` grammar (11.8 gives a function list only: no literal syntax, precedence, typed-value literal form, division-by-zero or domain-error result, string comparison, `int`/`dec` coercion, `log` of a negative); `rank` with `scope: group` (grouped by what); `rank` `missing: exclude` with `take`; the `group` member-key fingerprint algorithm; `join` result key for downstream tie-breaking; `mean`/`median` rounding into decimal; the result fingerprint (row order, column order, key mapping); assertion `rank` when `row` is absent; `value` with `lt`/`gt` on strings or dates; `between` `expected` shape; `tol` with `ne`; the `describe.rs` sentence template per op (item 92 puts "description text" in `expected.json`, so the Python checker must reproduce it byte for byte). A separate session implementing from the spec alone will make each choice independently of the Rust author, which defeats D21. **Guidance:** a Checker semantics section (EBNF grammar, per-op evaluation rules, per-op description template, fingerprint definition, assertion edge cases), or make writing `checker.md` an explicit phase with required contents enumerated and owner review before item 87.

### plan-1-1: the Verification Plan names binaries and checks that gate A cannot produce, and the CI grep is unsatisfiable as written
**Scope:** item 6 (line 1164), item 7 (1165), Build (1292), Static analysis (1294). Duplicates: style-1-8, impl-1-12 (part).
`release.yml` builds `tm`, `tm-agent`, and `tm-bridge`; the Build step targets `tm` and `tm-bridge`; `check-owner-cap.sh` builds `tm-agent` and `tm-bridge` standalone; item 6 says the same job "passes trivially". Three accounts of one check. The rule "no crate other than `tm-cli` names `OwnerCap::new_for_cli`" is literally unsatisfiable since `tm-owner-cap` defines it. **Guidance:** limit gate A to `tm`; note the other binaries arrive in gates B and D; exclude the defining crate from the grep; reword item 6.

---

## Major

### impl-1-5 / plan-1-2: dependency choices left open
**Scope:** item 1 against items 15, 26, 31, 35, 38, 48, 49, 62, 79, 86; Coverage (1312); NFR line 1126.
Unpinned: exact `duckdb` version (and the `arrow` pin must match duckdb-rs's re-exported arrow major); 34-digit decimal type (`rust_decimal` caps at 28 digits; `bigdecimal` is arbitrary; neither in item 1); `statx`/`O_EXCL` binding (`rustix`, `nix`, `libc`); Parquet writer for the cache (`parquet` crate or DuckDB `COPY TO`); PDF page-tree crate; HTML template engine and Markdown renderer plus sanitizer (4.4 requires the viewer's sanitizer, none named); `tempfile`; `libfuzzer-sys`/`cargo-fuzz`; `assert_cmd`; reflink crate or `ioctl`; coverage tool for the 85 percent bar (no CI job measures it). `mimalloc` is pinned in item 1 before the spike that confirms it. **Guidance:** every crate and version in item 1; name the coverage tool and its job.

### impl-1-6: static view page set, URLs, content, and action-file UI are not specified
**Scope:** item 79; G3, G9; CUJ 4; 11.15.
Given: page families and one example path. Not given: naming per family, per-page query list (the claim page needs `claims.show`, `claims.evidence`, `claims.citing_rows`, `files.lineage`), the JSON block format and element ID the G9 parity test reads, template structure, how the agent-denied list is honoured, which actions the gate A UI offers. 11.15 lists `draft.promote`, `run.approve`, `claim.review`; only `claim.review` has a gate A operation. **Guidance:** a page table (path pattern, queries, sections, JSON block ID) and the gate A action set, with `apply.rs` refusing the other two by named error.

### impl-1-7: the reference project and the gate A exit criterion have no fixture or procedure
**Scope:** item 82 (`fixtures/`), item 95, gate A exit row (1101), G1, G3, G8, G9, G13. Duplicate: style-1-15.
"Three tables, ten claims" with no schemas, specs, sentences, steps, assertions, or expected verdicts, and it is not the reference project the measures name. The manual criterion has no acceptance step. **Guidance:** define the fixture concretely (or dictate `fixtures/README.md`), name the paper, its tables, claim count, and the sign-off checklist and where it is stored.

### impl-1-8 / plan-1-9(a): settings schema has no consolidated key list
**Scope:** item 81, item 74 (`settings.rs`), 5.5, OQ1-OQ5, 3.1, 2.3, 3.3, NFR.
Named keys: eight. Unnamed but required: OQ1 threshold, OQ2 takeover timeout, OQ3 abandon timeout, OQ4 four limits, OQ5 warn threshold and byte budget, login-node hostname pattern, DuckDB `memory_limit` and `temp_directory` override, login-node hashing limit, agent-denied list. Types, units, the `settings.changed` body, and whether settings come from the fold or a chain scan are unspecified; `E7001` unknown key needs a closed list. **Guidance:** a settings table (key, type, unit, default, validation, gate) and the `settings.changed` body.

### impl-1-9: batch builder and segment writer leave durability and framing decisions open
**Scope:** item 75, item 33, item 34, 11.3, 3.1.
Missing: whether `len` counts the last event's newline; whether the header is newline-terminated and excluded from `h`; fsync order (segment, segment fsync, `HEAD` rename, directory fsync); whether a batch may span a segment roll; who seals and packs; zstd level; `HEAD.offset` definition; `seq` within a batch versus `HEAD.seq`. `conformance/record/chains/*` will freeze each. **Guidance:** the exact write sequence and byte accounting in 11.3 or items 33/75.

### impl-1-10: kept-object store layout unspecified
**Scope:** item 38, item 40, 11.5.
Sharding under `objects/sha256/`, suffix, whether the address hashes raw or compressed bytes (must be raw), zstd level, temp-then-rename path, the verify step the Risks table names. **Guidance:** one paragraph with path pattern and sequence.

### impl-1-11: span scanner grammar is not defined
**Scope:** item 68, item 72, D10.
Only the example. Undefined: nested brackets, spans across line breaks, multiple attributes and order, quoting of `ref`, what a "heading class" is and what the scanner does with it, duplicate refs, fenced code versus inline code, the whitespace normalization steps. `tests/spans.rs` cannot be written. **Guidance:** grammar (regex or EBNF), heading-class rule, normalization order, fixture list.

### impl-1-12 / plan-1-5(c): CLI surface is not specified per command
**Scope:** items 83-84, item 86, section 12, CUJ 1 and 6.
Flag names and types, required versus optional, `--json` output, exit codes per error class, prompt wording, what `tm status` prints in gate A; `tm claim add` alone needs eight flags and the citation syntax for whole-file, page, and panel is undefined. Item 83 lists gate A objects but not verbs: `tm claim accept` is gate C per line 1089; `tm file restore` needs a retired file gate A cannot produce. **Guidance:** a per-command flag table, the gate A verb set per object, and the exit-code map.

### impl-1-13: `OwnerCap` API underspecified
**Scope:** item 43, item 6, item 8, rule 4. Duplicates: style-1-16 (item 43), plan-1-17.
"No `Clone` across crate boundaries beyond what the Core needs" is not a decision. `&OwnerCap` or by value (NFR says the viewer "drops" it) and how `tm-core` threads it through `read_in` are unstated. **Guidance:** no `Clone`; `&OwnerCap` everywhere; fix the grep scope.

### impl-1-14 / plan-1-3 / plan-1-6: Verification Plan commands disagree with the items and the design
**Scope:** lines 1290-1312; items 6, 24, 59, 62, 82, 87, 89. Duplicates: style-1-6, style-1-7, plan-1-14, plan-1-15.
(a) Differential tolerates `log` and `exp` only; G14, D21, and 11.6 add `mean` and `median`. (b) Smoke list in item 6 differs from the table. (c) `cargo test -p tm-core --test scenarios` needs `tests/scenarios/main.rs` with `mod` declarations. (d) `cargo insta` and `cargo fuzz` need `cargo-insta`, `cargo-fuzz`, and nightly, while item 3 pins stable; no job installs them. (e) The NFS lock test and NFS-loopback wrapper budget need a privileged CI step item 6 lacks. (f) `python3 -m pytest` needs pytest, outside the standard-library restriction. Pass conditions "Green", "readable style", "description matches its assertions", "no unreviewed snapshot change", "panics are bugs" are not conditions. **Guidance:** reconcile each pair; add tool-install steps; measurable pass conditions.

### impl-1-15 / plan-1-7: tampering and scenario cases are not enumerated; three gate A exit criteria have no test
**Scope:** items 41, 91, 82, 42, 31, 52; Verification rows; gate A exit row.
"Each tampering case" points at G13's four plus 3.1's torn tail and fork, never listed with expected report names, so `tamper/*/` names and `verify.rs`'s result enum cannot be derived. Scenario tests have no assertions. G9 parity has no test; G1's scripted scan has no script; G2's Python rejection of invalid steps has no command consuming `conformance/formats/steps/invalid/`. CUJ 5 and 8 presuppose sessions and receipts gate A lacks; no adaptation stated. **Guidance:** a case table per suite (name, setup, command, expected code or label); add the three missing tests.

### impl-1-16: `tm doctor` detection methods unstated
**Scope:** item 85, 3.4, 2.3. Duplicate: plan-1-5(b) (doctor logic sits in `tm-cli`, against rule 3; the rename-visibility probe is missing).
`statfs` magic numbers and which count as network; whether the `O_EXCL` and rename probes run at `tm init` or only `tm doctor`; what "clock skew, local only" measures. **Guidance:** list probes with mechanism and pass rule; move the logic to a Core operation.

### impl-1-17: parse path decisions missing
**Scope:** item 47, item 48, item 16, 11.6, 11.7.
How `format: parquet` is read; how the Parquet cache is written (fixes byte layout); how "largest scale seen" is computed; spec-type-to-Arrow mapping; `tm-files` opening DuckDB a second time with duplicated settings. **Guidance:** reader per format, cache writer, mapping table.

### impl-1-18: audit report schema and `C<n>` allocation unspecified
**Scope:** item 80, item 64, CUJ 8.
No `audit.json` shape although `audit.completed` records its hash; `--since LAST` semantics unstated; `C<n>` counter source (chain scan for last `claim.added`, or kept in `HEAD`) unstated. **Guidance:** JSON shape and counter rule.

### plan-1-4: gate A operations emit event types whose body files item 14 omits or marks reserved
**Scope:** item 14 against section 9 and 11.2.
`lock.taken_over` (no `events/lock.rs`), `engine.upgraded` (no `events/engine.rs`), `journal.head_recorded`, `journal.event_quarantined`, `receipt.ingested` (`journal.rs` reserved; `receipt.` in no namespace list), `report.registered` (no namespace, no file). Reserved types cannot be written under item 14's closed enum. **Guidance:** add the files or move the operations out of gate A per the Stage 8 move list.

### plan-1-5(a): `report.check` has no ops file
Section 9 line 668 and `tm report check` (line 904) are gate A; no `ops/report_check.rs`.

### plan-1-8: the NFS lock test the architecture promises is absent
3.1 says `tests/lock.rs` includes an NFS-mount case; item 42 has two contending processes and a simulated epoch bump only.

### plan-1-9(b,c): closed lists the implementer must invent
Item 66's "gate A subset" of 5.6 is unlisted; Coverage's "every `E` code produced by a test" is unscoped to gate A codes; `E3103` and `E3104` have no named producer.

### plan-1-10: background hashing has no executing process in the plan
`file_register.rs` must "schedule background hash" and later emit `file.hashing_finished`; no item says which process does it. (Stage 5 finding 2 and Stage 8 finding 2 are the design-level form of this.)

### plan-1-11: the stat cache and the per-command project resolution have no plan item
3.4's stat cache (required by `tm status` and item 30) is created by nothing; item 76 omits the path resolution order, `PRAGMA integrity_check`, and rebuild trigger; reading `project.json`, the binding, and `record.json` with `E2004`/`E2005` is assigned to no file.

### plan-1-12: `docs/errors.md` is "generated" by nothing
No generator item and no CI currency check.

### style-1-1: ten D-citations do not match the register row
Items 10 (D24 does not point at `musl-spike.md`), 11 and 25 (D14 says nothing about in-house canonical JSON or RFC 8785), 12 (D14 has no ID types), 16 (D8 is query steps, not parse specs), 44 (D5/D22 do not state identity rules), 50 (D19 has no provider checksums), 60 and 62 (D21 has no property tests or fuzzing), 82 (D14 does not say readable scenario tests). Duplicates: plan-1-13, plan-1-16.

### style-1-4: "lease" has no glossary entry and the register says the lease was dropped
`lease.rs` (item 74) and the `lease` command (item 83); D1 and D15 replaced the lease with a lock. Rename to `lock`, or add a glossary row.

### style-1-3: glossary words misused in the plan
Item 29 "a flag for filesystems" (a setting); items 49, 51 "panel labels"; item 37 "idempotent claim by hash"; items 28, 38 "hash" for file and manifest digests; item 55 "result fingerprint" for a hash of rows.

### style-1-2: "Record" used for the directory (line 1151) and the library (item 75) against the glossary split.

### style-1-5: mixed verb forms across item descriptions (noun phrases, third-person verbs, imperatives). Pick one.

---

## Nit

- **impl-N1:** `E1001` appears only in item 11; no `E1xxx` list; `rule` and `owner_command` text per code is nowhere. Item 21 should state the catalog is the union of section 9's codes plus the formats codes. (Duplicate: plan-1-16.)
- **impl-N2:** OQ7's harness variable list: say it is exactly `[CLAUDECODE]` in gate A or list it.
- **impl-N3:** items 44 and 50 cite "hypothesis decision 7/9", which the implementer does not have; restate the identity and checksum-format rules (which MD5/SHA-256 list formats).
- **impl-N4:** item 60's "filter-then-sort equals sort-then-filter" holds only when the filter does not depend on rank; state the generator constraint; the 1,000-case count is only in the table.
- **impl-N5:** item 95's guide is the exit-criterion procedure's only instruction; list its sections.
- **plan-N1 (plan-1-18):** three `audit.rs` and two `settings.rs` in `tm-core`; state the split.
- **plan-N2 (plan-1-20):** item 9 has no Delete item for the spike; items 84 and 79 are globs.
- **style-N1 (style-1-10):** circular rationales ("same", "as above", "one encoding everywhere"); "the three hashing tiers", "the cross-machine findings", "adopt-as-written" (spelled differently from the register) are unlabeled pointers.
- **style-N2 (style-1-11):** B/M labels in items 16, 20, 28, 46, 47, 49, 66; cite register rows or sections.
- **style-N3 (style-1-12):** rhetorical rationales in items 31, 42, 95.
- **style-N4 (style-1-13):** missing serial "and" in file lists (items 14, 20, 74, 77, 87, 88) and lines 1303, 1310.
- **style-N5 (style-1-14):** index budget checked twice (item 82 scenario and `bench-index.sh`).
- **style-N6 (style-1-15):** gate A row's two manual criteria (spike recorded, paper re-derived) absent from the Verification Plan; line 1314 sends the reader to "Future Improvements" for present-tense criteria.
- **style-N7 (style-1-16):** history and jargon: item 57 "formerly called the profile", item 20 "DVC-shaped", item 44's parenthetical.
- **style-N8 (style-1-17):** `draft.rs` should be `proposal.rs` per the glossary.
- **style-N9 (style-1-18):** "honours" versus US spelling elsewhere.

## Verified consistent (no finding)

Crate names between 4.1 and the plan; conformance directory names across items 24, 41, 59, 90-92; test target names for `tm-formats`, `tm-record`, `tm-checker` against the Verification commands; script names in item 8; 64 MiB segments; 60 s budget; 10,000 differential cases; 5 s heartbeat; DuckDB engine settings; the three distributions; item 77's twelve read families against section 10; item 32's layout against 3.3, 11.3, 11.5. No dashes; no "ingest" in scope.

---

# Stage 9: Implementation Readiness Test (round 2, after the plan rewrite)

*   **Target:** `toulmin_v1_design.md`, DetailedRange lines 1360-1548, plus the new 11.16-11.22 (lines 890-1092)
*   **Reviewers:** Implementation Readiness, Codebase Details (plan-consistency mode), Style. All completed with the settled list.
*   **Round verdict:** not clean. 3 Blockers (one found by all three reviewers), 21 Majors after merging, 20 Nits. Re-audit cap for Stage 9 is now reached.

## Blocker

### impl-2-B1 / plan-2-B1: `tm-files` depends on `tm-checker` (item 47), breaking rule 1 and the phase order
Item 47 parses CSV "through a shared `tm-checker::engine` helper". P3 builds `tm-files` and its checkpoint is `cargo test -p tm-files`, but `tm-checker` is P4, so P3 cannot compile; and 4.2 rule 1 forbids libraries calling each other. **Guidance:** a new shared crate `tm-duckdb` (connection settings, version and extension capture) in 4.1 beside `tm-formats` and `tm-fsfacts`, built in P2; items 47 and 54 both use it.

### impl-2-B2 / plan-2-M5 / style-2-1: the citation object encoding is defined nowhere
11.16 `claim.added` says "a list of 11.x citation objects"; no such subsection exists; item 19 gives Rust variant names and the CLI spelling only. It is part of a hashed body, stored in the index, in the conformance envelopes, and reproduced by the Python cross-check. **Guidance:** 11.23 "Citation objects" with one key list per kind; cite it from 11.16, item 19, and item 90.

### impl-2-B3: `scripts/differential.sh` has no Rust entry point before `tm` exists, and `tm` has no project-less Checker command
P5's checkpoint cannot run. Generator ownership is split between items 87 and 89. The two-CPU-generation requirement of 11.18.2 has no CI mechanism. **Guidance:** a `tm-checker` example binary `examples/refcheck.rs` with `refcheck.py`'s argument shape, added to item 59; the generator belongs to item 89 (Rust side) with its distribution stated; two named runner classes for the differential job.

## Major

- **impl-2-M1 / plan-2-M1 / style-2-3: phase membership.** Items 6-8 in no phase; item 72 in P6 (should be P7); item 79 in both P7 and P8; item 94 must exist from P1 because item 25's CI diff fails otherwise; P5 "Item 87, 89".
- **impl-2-M2: P3's checkpoint names fixture tables that exist only in P8.** Create a workspace-level `fixtures/` in P3 that item 82 reuses.
- **impl-2-M3: 34-digit decimal arithmetic cannot come from DuckDB SQL alone** (division returns DOUBLE; `DECIMAL(38,s)` products overflow), and the rounding mode is unstated, so Rust and Python will disagree on last digits. Decide where `derive` arithmetic runs and state precision and rounding (half-even) in 11.18.2.
- **impl-2-M4: `log` base.** DuckDB `log` is base 10, Python `math.log` is natural. State "`log` is the natural logarithm, translated to DuckDB `ln`."
- **impl-2-M5 / style-2-8: description templates are not reproducible byte for byte.** `{dir}`, `{ties}`, `{scope}`, `{k}`, thousands separator, list conjunction, and expression normalization are unpinned; `sort.dir` values are not enumerated in 11.8.
- **impl-2-M6 / plan-2-M2(flags): fold-written versus scan-derived flags are not distinguished**, so item 70 cannot be written; `possibly changed (unconfirmed)` is missing from item 66 and `retired` is present though unreachable in gate A.
- **impl-2-M7: G2's Python half has no code.** `reference/formats/` covers canonical JSON and IDs only; nothing validates `steps/invalid/` in Python.
- **impl-2-M8: G13 idempotent audit and G3 page content have no test.** Add `audit_idempotent.rs` and `claim_pages.rs`.
- **impl-2-M9 / plan-2-M7 / style-2-2: `tm file lineage` missing from item 83**; `checker.preview_steps` and `describe_steps` have no CLI surface, against G9.
- **impl-2-M10: D21 independence undercut.** Item 87 allows reading `expected.json` after the first run with no rule for differences; who writes `expected.json` is unstated; the Python checker needs canonical JSON for three fingerprints but `checker.md` excludes 11.1. State: `expected.json` written by hand before either checker runs; differences recorded in `docs/decisions/checker-diffs.md` and ruled against `checker.md`; `canonical_json.py` may be imported.
- **impl-2-M11 / plan-2-M9 / style-2-10: Coverage row.** `--workspace` contradicts the four-crate scope; the "coverage log" for error codes is created by nothing; `symbol-versions.sh` has no stated criterion; "Each crate's own rules" says nothing.
- **impl-2-M12: fixture claims do not cover the eight ops and five assertion kinds; `claims/*.json` is an undefined composite format; `figure.pdf` and `figure.png` are not listed.**
- **impl-2-M13: `table spec` cannot express `nan`, `inf`, header-less files, or per-column missing tokens; `settings set` has no list syntax.**
- **impl-2-M14: `F7@v2#key=GeneX` is ambiguous.** Define `#<column>=<value>` per key column in spec key order, with escaping.
- **impl-2-M15 / plan-2-M2(spans) / style-2-4: 11.22's `section_kind` is in neither the `report.registered` body nor the `spans` table**; `inline_code` cannot be folded from the event body either.
- **plan-2-M2: 11.17 lacks columns section 10 queries need:** `claims.show` (sql, steps, fingerprints, error on `verdicts`); `claims.evidence` (per-step rows must be recomputed from the parse cache, unstated); `claims.citing_rows` (needs a `claim_keys(ref, version, key_tuple)` table); `claims.history` (no ref on `events`; retract and supersede event IDs unstored); `audit.last` (no `audits` table); Core-written tables not named in item 76.
- **plan-2-M3: `stat_cache` contradicts 11.17** (not rebuildable; dropped on rebuild; needed by `--since`). Own DDL, excluded from the drop.
- **plan-2-M4: `chain.torn_tail_truncated` has no producer item.**
- **plan-2-M6 / style-2-4: item 20's catalog excludes `E6004` and `E8102`.**
- **plan-2-M8: item 29's "setting-driven mode" has no 11.19 key; item 72 reads `harness.env_vars` in gate A though 11.19 marks it gate B.**
- **plan-2-M9: tests and scripts the Verification Plan names that no item creates:** the one-million-event chain generator; the spec-drift CI diff (and the design document's repository path); the `docs/errors.md` CI job; the glibc fallback build in CI.
- **plan-2-M10: the 8 MiB main-thread stack is set only in the spike**, not in `tm`.
- **plan-2-M11: item 9's Delete makes the P0 checkpoint unsatisfiable after the deleting change.**
- **plan-2-M12 / style-2-6: the 11.21 reflink sentence cannot be implemented** ("stores nothing" has nothing to reflink); 11.21 step order and 11.3's "at most 64 MiB" disagree with the oversize-batch rule.
- **plan-2-M13: `doctor` probes have no library item** (`statfs`, exclusive create, two-process rename visibility described only in the `tm-cli` renderer).
- **style-2-5: the fold contract states idempotence two ways and the second (`INSERT OR IGNORE` everywhere) is wrong** for tables keyed by ref, file, subject, or key.
- **style-2-7: glossary violations in new text:** "kept and ignored" (11.22); `repo_root_fingerprint`, `final_rows_fingerprint`, `key_map_fingerprint`, `member_keys_fingerprint` digest derived JSON, not files (rename to `_hash`, or widen the glossary entry); "Record write sequence" heading.
- **style-2-2: Rationale citations that do not match:** item 1 "the Zero-Decision Rule" (undefined in the document); item 30 (D19's tiers are per command, not per file); item 32 (3.3 is Receipts); item 93 (D21 covers `checker.md` only).
- **style-2-9: verb-form drift** in items 23, 25, 32, 34, 47, 55, 59, 69, 75, 80, 85, 89, and tails of 39 and 83.

## Nit

impl-N1 (`rank.group_by` absent from 11.8); impl-N2 / plan-N2 (item 83 flags: `file hash --again`, `file keep --fingerprint-only`, `claim review --outcome --note`, `claim retract --reason`, `claim supersede ...`, `file rename`; `file restore` has no reachable gate A path); impl-N3 (item 20 wording); impl-N4 (`login_node.pattern` has no gate A reader); impl-N5 (fuzz `<target>` placeholder); impl-N6 / plan-N7 (`int` beyond 38 digits is `E3102`); impl-N7 (spec drift and errors.md CI rows); impl-N8 (doctor probes to Core); impl-N9 (lock-time `HEAD` rewrite missing from 11.21; who does the fencing re-read); impl-N10 (`--since LAST` literal or ID); impl-N11 (which binary the smoke containers run); impl-N12 (`isatty` on which descriptor); impl-N13 / plan-N1 (`claims.state` values); plan-N3 (`tm-data` block layout unspecified; shared `actions.js` versus "page-local script"; `audit.html` path collision between `tm audit --out` and `tm view --out`); plan-N4 (segment size wording); plan-N6 (item 10 edits this document, unmarked Modify, no repository path); plan-N8 (6.1 panel labels at identity level versus version level everywhere else); plan-N9 (`settings_show`/`unset` files); plan-N10 (`changed_outside_run` versions in gate A: does `tm status` add versions); style-2-11 (six Oxford comma omissions in 11.18.2 and item 85); style-2-12 (hand-waving phrases in 11.16-11.22); style-2-13 (item wording: 9, 91 empty slot, 15, 56, 82, 29); style-2-14 (11.19 validation cells).

---

# Stage 9: Implementation Readiness Test (session 2, round 1, after round-4 fixes)

*   **Target:** `toulmin_v1_design.md`, DetailedRange lines 1384-1582, plus 11.16-11.23 (lines 892-1116)
*   **Reviewers:** Implementation Readiness, Codebase Details (plan-consistency mode), Style; all with the owner's three focus questions and the do-not-reopen list.
*   **Round verdict:** not clean. 3 Blockers (all Implementation Readiness), 29 Majors after merging, 35 Nits. No round-4 content had been audited before this; most findings are seams in it.

## Owner's three questions, answered

1. **Do 11.16-11.23 give plan items 11-97 what they need?** Mostly. 42 items verified as finding what they cite. The gaps are listed below; the two that block are `IndexTx` having no crate (B1) and the steps schema lacking a `from` table and a `join.on` shape (B2).
2. **Do the ten fixture claims exercise every 11.18 rule?** No. They cover each op and assertion kind once. Not covered: three of four `ties` values, `missing: exclude`, `rank scope: all`, `sum`/`min`/`max`/`median`, the second join kind, `between`, `ne` with `tol`, NaN and Inf, the three domain rules, `log`/`exp`. Claim 10 ("cells with a composite key") is impossible on the listed tables, which all have single keys. The 60 conformance cases of item 92 are where exhaustive coverage belongs; 82a should claim only "every op and assertion kind" and gain a composite-key table.
3. **Is `docs/spec/checker.md` enough for an independent Python checker?** No. Seventeen facts a Python author would need and not find are itemised under B3: the `expected.json` schema itself, CSV reading rules, three-valued logic for `and`/`or`/`not` with missing, `-5` tokenization, `{expr}` re-serialization, return types, the `E5002` condition, `ties: max` formula, `group` missing placement and tuple sort, `join` column order, per-op column order, assertion `row` shape, `tol` with ordering operators, template variable-length parts, and the `field` path syntax for `E6002`/`E6004`.

## Blocker

### impl-s2-B1: `IndexTx` has no home crate; P3 and P6 cannot compile
11.17 defines `IndexTx` and rule 6 has every library expose `fn fold(&mut IndexTx, &Event)`, but the only file that could define it is `tm-core/src/index/mod.rs` (P7). `tm-files` (P3) and `tm-claims` (P6) must name it, so they depend on `tm-core` (a cycle) or on nothing. Same class as session 1 round 2's B1. **Guidance:** a tiny `tm-index` crate (the `IndexTx` wrapper and `rusqlite`) in 4.1 beside `tm-duckdb`, built in P2.

### impl-s2-B2: the steps schema has no `from` table and `join.on` has no JSON shape
With two tables (every `join`) the start table is undefined, and canonical JSON sorts keys so "first entry" is not a rule. `on` is "key pairs" with no shape. Items 17 and 87 will each pick one; no join conformance case can be written. **Guidance:** required top-level `"from"`; `on` as `[{"left","right"}, ...]`.

### impl-s2-B3: `docs/spec/checker.md` is not enough for the Python session (17 facts)
Listed in answer 3 above, with the reviewer's suggested defaults in the agent report: `expected.json` schema `{"steps":[{"op","rows_out","description"}],"final_rows","final_rows_hash","key_map_hash","assertion":{"outcome","observed","expected"},"error"}`; version-handle to `tables/*.csv` mapping; CSV quoting (RFC 4180), bool tokens, date forms; three-valued logic choice; `-5` tokenization; AST re-serialization rule; return types for `abs`, `log` of int, `coalesce`, `=` on bool, date versus datetime; `E5002` condition; `ties: max` as count of rows at or before; `group` missing placement and tuple sort by canonical JSON string; `median` interpolation; aggregate type rules; `join` right-key inclusion, left fill, duplicate names; per-op column order; assertion `row` and `compare_rows` shapes; `tol` with ordering ops; `between` reversed bounds; `observed` when absent; template variable-length rendering; `field` path syntax. **Guidance:** a new 11.18.5 "Conformance case format and remaining rules", included by item 93; P4's sign-off gated on it.

## Major (merged across reviewers)

- **impl-M1 / plan-M9:** quantitative claims' citations: does `claim_add.rs` synthesize `cells` citations and `claim_keys` from the key map, or store only CLI input; `E6003` (refuse) versus 7.3 (record `fails`) for an absent key; whether `steps: null` with an assertion is allowed.
- **impl-M2:** spans have no write path after `report.registered`; CUJ 6 registers the report before the span exists; `reports.spans`, the two span flags, and "records the sentence" have no data source. Make `spans` scan-derived (like `stat_cache`) or add a `report.spans_recorded` event.
- **impl-M3:** `settings.changed` has no encoding for list-valued settings; 11.6 has no list type.
- **impl-M4:** `snapshot` and `manifest` objects inside `file.version_added` have no schema.
- **impl-M5:** interleaved Rust and DuckDB steps: stored `sql` form for pipelines with Rust steps in the middle, order carriage between engines, derived-column scale, the `E5002` boundary.
- **impl-M6:** text form of computed decimals (`1.10 * 2`, `10 / 4`, negative zero, exponent forms) and `float64` to string (Rust `0.00001` versus Python `1e-05`) unpinned; `final_rows_hash` compares these exactly.
- **impl-M7:** fixture coverage (answer 2); claim 10 impossible; data features (missing, NaN, ties, unmatched join rows) unstated; `args.txt` format.
- **impl-M8 / style-10:** `field` path syntax for `E6002`/`E6004`; tolerance magnitude never quantified beyond "last digit"; "heading normalization" undefined in the Spec drift row.
- **plan-M1:** P7 lists `view_export.rs`, which needs P8's templates; `bench-index.sh` has no entry point in P7; `verdict_data_changed` is in `tamper/` but `tm-record` cannot detect it.
- **plan-M2 / style-8:** item 5a (a rule about a P8 file) and item 91 in no phase; P8 cites "item 82's checklist" (it is 82a's).
- **plan-M3:** `claim_events` has two writers (Core per 11.17 and item 76, Claims fold per item 70); file-subject labels in `flags` (`removable`, `restored`, `evidence not kept`) derive from Files events but item 66 assigns them to the Claims fold; `removable` is already a `files` column.
- **plan-M4 / style-3 / impl-N1:** `claim.revised` body lacks `cited` and `citations`, which 11.23 says it carries and `claim_revisions` requires `NOT NULL`.
- **plan-M5:** item 37 has `tm-record::scan.rs` read the Core's index, inverting the crate graph.
- **plan-M6:** the gate A half of `action_file.rs` (the `claim.review` subset) has no `tm-formats` item; `chain_head` and `exported_at` are not said to be on the page.
- **plan-M7 / style-14 / impl-N4:** the `tm-data` block "keyed by query name" cannot hold repeated queries; the parity test has no query-to-command table (`flags.list` is `tm status --flags`, which takes the lock).
- **plan-M8 / style-4:** section 12 lacks `table describe` and `table preview --steps`; `--cited-keys` is in 11.23 but not item 83 and its file format is undefined; `cells.columns` has no CLI spelling; `--cited HANDLE...` should be `CITATION...`.
- **plan-M10:** the Error-codes test diffs against a "gate A code list" nothing defines (no gate attribute on the catalog).
- **plan-M11 / style-10:** `reference/checker/tolerance.json` is consumed by two commands and created by no item.
- **style-1:** 11.16 "declared but reserved" versus item 14 "not declared in gate A".
- **style-2:** item 59's example "writes `expected.json`" versus item 87's "never written by the Rust Checker".
- **style-5:** item 66 calls labels flags and stores them in `flags`; `expected-missing` is both scan-derived and "permanent".
- **style-6:** item 43 "the Record append" (library, not content).
- **style-7:** 3.4 says the stat cache is "a file beside the index"; 11.17 says a table in it.
- **style-9:** Scenario row counts eight tests; item 82 lists ten.
- **style-11:** `ties: max` "inverse RANK" has no SQL meaning; `between` with `tol` unspecified per end; `compare_rows` never states its comparison.
- **style-12:** 11.18.4 placeholders still unrendered: `{col}`, `{as}`, `{left_col}`, `{right_col}`, `{group_by}`, `{key}` as lists; `sort` fixed at two columns; ", ..." versus the list rule; spacing around `not`/`and`/`or` and after commas; singular "1 rows".
- **style-13:** Rationale citations: item 26a (rule 2, not rule 1); item 74 ("one file per row" is false for the doctor/lock/settings row); item 86 (G12 is `smoke.sh`'s).

## Nit (35, grouped)

Reserved `file.hashing_finished` leftover; "eight" versus ten tests; Conformance: formats row names the wrong Python command for invalid steps; `checker-spec-review.md`, `checker-diffs.md`, `tolerance.json` created by no item; 3.1 word order on truncate-then-append and which function does the fencing re-read; `csv_parse` fuzz target lives in the wrong crate; `tm audit --out` has no template item for its own `audit.html`; item 77 ignores the `Unconfirmed` outcome and `--hash`; `reports.spans` after `update_text` (which sentence); item 83 lacks list filter flags and `files.bytes`; NFR budgets (incremental index, disk scan) with no test row; Spec drift cannot be an equality if `checker.md` adds the case format; no `citations/` conformance cases or escape tests; the gate A Markdown sanitizer module unnamed; `assertions/` needs a valid/invalid split; `E6002` has two producers; `project.init`'s `settings.changed` key; `evidence.detail` shape; `spans` primary key fails for two refs on one line; out-of-order `#column=` input; `report_hash` naming; 11.16 restates glossary and notation; `stat_cache` "never dropped" twice; `log2`/`log10` in the domain row; date versus datetime comparison; 11.23 circular pointer; stray `---` at line 1101; seven items in sentence form; nine free-text rationales; double-counted `verdict_data_changed`; "and" placement in item 6; "CUJ 5, 6, 8"; Coverage "Passes"; runner class wording; design path `docs/design/` versus `docs/designs/`; "makes safe"; items 57 and 43 cite D12 and D2 where G2 and rule 4 apply.
