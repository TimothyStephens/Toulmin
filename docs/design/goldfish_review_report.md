# Goldfish Annotated Review Report

**Status: resolved** (round 1 fixes applied 2026-11-09; escalations answered: E1 = working tree is mounted, so D28 stands and all "clone" wording changed; E2 = automatic takeover kept, waiting commands name the holder; E3 = tolerance extended to `mean` and `median`). Pre-edit copy: `.toulmin_v1_design.before_round1.md`.

**Target:** `toulmin_v1_design.md` (FeatureRange, lines 1-1053)
**Stage:** 7 (Critic Review), round 1
**Source findings:** `critic_report.md`

Summary of categorisation: 1 Blocker (PROPOSE FIX), 30 Major (23 PROPOSE FIX, 3 ESCALATE, 1 CONTEST, 3 PROPOSE FIX in part with an escalated choice), 27 Nit (14 PROPOSE FIX, 13 DECLINE).

Escalated items need your knowledge, not just approval. They are listed first.

---

## Escalations (your call, not mine)

### [E1] design-1-4 [`Design Critic`] - Severity: Major
*   **Affected Scope:** lines 113, 226, 296, 516, 813, 978 (CUJ 3, Setup 1, Receipts, 5.6, 11.14, D28)
*   **Discrepancy Description:** D28 labels every project-inbox receipt arriving during any session as "possibly agent-issued" because "the project tree is mounted in agent sandboxes"; elsewhere the agent works in a separate clone. If it is a clone, D28 permanently mislabels legitimate owner runs; if the working tree is mounted, the handshake and attribution story need rewriting.
*   ***AI Suggestion:*** ESCALATE
*   ***Justification:*** The document says "clone" in four places and "project tree is mounted" in one, but what agentbox actually mounts is a fact about agentbox, which is outside every file the reviewer could read. If agentbox mounts a fresh clone, the right fix is: restrict the D28 label to unisolated sessions, make it a flag cleared by `tm run review --note`, and change D28's reason. If agentbox bind-mounts the owner's working tree, D28 stands and the "clone" wording in CUJ 3, Setup 1, and the handshake's `project_root` must change instead. Tell me which, and I will apply the matching edit.

### [E2] design-1-2(f) and zoom-out 8 [`Design Critic`, `Zoom Out`] - Severity: Major
*   **Affected Scope:** lines 87, 278-280, 650 (OQ2, Main chain Takeover, `lease.break`)
*   **Discrepancy Description:** Automatic lock takeover during ordinary commands means `tm status` can silently block for 5 minutes after any CLI crash; the Zoom Out reviewer argued the heartbeat thread and automatic takeover are overkill for one owner.
*   ***AI Suggestion:*** ESCALATE
*   ***Justification:*** Both reviewers converge on removing automatic takeover and keeping epoch fencing plus a manual `tm lease break`. That is a change to D15 as recorded, which you settled. The mechanical parts of design-1-2 (a-e, g) are fixed below regardless. Choose: (a) keep automatic takeover as designed; (b) drop automatic takeover, keep heartbeat so `tm lease status` can show staleness, commands fail fast naming the holder; (c) drop both heartbeat and automatic takeover.

### [E3] design-1-15(a) [`Design Critic`] - Severity: Major
*   **Affected Scope:** lines 765, 779, 1028, G14 line 67
*   **Discrepancy Description:** `mean` and `median` in `group` are not covered by "decimal arithmetic in derive"; DuckDB's `AVG` over `DECIMAL` returns `DOUBLE`, so `log` and `exp` are not the only tolerated differences.
*   ***AI Suggestion:*** ESCALATE (the rest of design-1-15 is PROPOSE FIX below)
*   ***Justification:*** Two defensible answers: extend the last-digit tolerance to `mean` and `median` (one line in 11.6, 11.8, and G14), or compute aggregates in decimal outside DuckDB (a Checker design change that weakens "the SQL is the full computation"). If you give no preference I will apply the tolerance extension, since it is reversible and does not change the architecture.

---

## Prioritized Findings & AI Suggestions

### [Issue 1] design-1-1 [`Design Critic`] - Severity: Blocker
*   **Affected Scope:** lines 58, 196, 313, 328-331, 348, 429, 1030
*   **Discrepancy Description:** `cargo tree -p tm-agent` must contain `tm-owner-cap` because `tm-core` names the type and `tm-agent` depends on `tm-core`; `read_in` and `claim.check` write the main chain without `OwnerCap`; the batch writer and lock live in `tm-core`.
*   **Rationale:** The G5 measure and the threat row "Prevent / None" rest on an unachievable check.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** The reviewer is right on the crate graph. The design's real controls are the tool surface and the mount list; the document should say so and make the CI check one that can pass.
*   ***Proposed Edit:***
    *   Line 313 (4.1 row): "The `OwnerCap` type (private field, no public constructor) and, behind the cargo feature `construct`, its only constructor. `tm-cli` enables the feature; no other crate does."
    *   Line 348 (rule 4): replace the last two sentences with: "Owner operations take an `OwnerCap` argument, and so does the Record's main-chain append, so `read_in` and system re-checks pass through the same gate (the CLI holds the capability for the whole command). Every crate may name the type; only `tm-cli` may construct it. CI fails if the `construct` feature is enabled anywhere in `tm-agent`'s or `tm-bridge`'s dependency tree (`cargo tree -p tm-agent -e features | grep 'tm-owner-cap feature \"construct\"'` must be empty). This is defence in depth: the primary controls are that `tm-agent` exposes no owner tool and that the record is not mounted in the sandbox."
    *   Line 58 (G5 measure): replace "CI fails if the agent tool server's dependency tree contains the owner-capability crate" with "CI fails if the owner-capability constructor feature is enabled in the agent tool server's or bridge's dependency tree".
    *   Line 196 (glossary): "A Rust value required by every owner operation and by every main-chain append; constructible only in `tm-cli`."
    *   Line 429 (threat row): V1 control becomes "No owner tools exist in `tm-agent`; the record is not mounted; `OwnerCap` is unconstructible outside `tm-cli` (checked in CI)". Remaining risk: "None within the stance for container sessions; unisolated sessions can run `tm` directly (next row)".
    *   Line 618 (`read_in` Permission): "Lock held, epoch valid, `OwnerCap` from the calling command". Line 636 (`claim.check` Permission): "`OwnerCap` from the calling command (system-initiated)".
    *   Line 1030: "the agent tool server cannot construct one" stays; add "(the constructor is feature-gated to `tm-cli`, checked in CI)".
    *   Gate A item 6 and item 43 (lines 1069, 1118): change the check description and the constructor list to match (DetailedRange edit, applied for consistency only).

### [Issue 2] design-1-2 (a-e, g) [`Design Critic`] - Severity: Major
*   **Affected Scope:** lines 253, 271, 278-282, 435-437, 721-723
*   **Discrepancy Description:** `O_EXCL` atomicity untested; check-then-act fencing window on NFS attribute caches; `O_APPEND` not atomic across clients; no `HEAD.offset` check; torn-tail truncation can remove valid batches; heartbeat write mechanism unstated.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** Each point is a real gap in the stated mechanism; the fixes are rules, not new machinery. Part (f) is escalated as E2.
*   ***Proposed Edit:***
    *   Line 279 (Epoch and fencing), append: "Taking the lock, or breaking it, writes the new epoch to `HEAD` by write-then-rename before anything else. Before every batch append the CLI re-reads `HEAD` with the directory's attribute cache bypassed (open the file fresh, `O_DIRECT` where the filesystem allows, otherwise after an `opendir`/`closedir` of `main/`), and also checks that the open segment's length equals `HEAD.offset`; a mismatch refuses the write and reports a fork."
    *   Line 278 (Lock), append: "The heartbeat is refreshed by rewriting the lock file in place, never by rename, so the inode created with `O_EXCL` is the one every reader checks."
    *   Line 282 (Torn tail): replace with: "A batch that lies beyond `HEAD.offset` and whose declared length is not satisfied or whose hash does not match is a torn tail. Readers stop before it. The next locked command truncates it after writing a `chain.torn_tail_truncated` event in the new batch. Anything inside `HEAD`'s range that fails verification is corruption, not a torn tail: it is reported by `tm audit` and never truncated."
    *   Line 263 / section 2.3, append to the rules list: "the record directory must support atomic exclusive create and atomic rename; `tm doctor` tests both on the record path and refuses filesystems that fail (NFSv2, some CIFS and FUSE mounts)."
    *   Lines 435-436 (threat rows "Two `tm` commands write at once", "Paused writer wakes after takeover"): Detect or prevent becomes "Prevent on filesystems that pass `tm doctor`; otherwise detect (fork, by `tm audit`)"; Remaining risk becomes "A woken writer that appends inside the attribute-cache window on a misconfigured mount produces a detectable fork, never silent data loss".

### [Issue 3] design-1-3 [`Design Critic`] - Severity: Major
*   **Affected Scope:** lines 228, 298, 438, 1163
*   **Discrepancy Description:** Index defaults to `~/.cache` (NFS home on clusters); "per login node" is false; threat row overclaims; owner operations' target resolution source unstated.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Proposed Edit:***
    *   Line 298: replace the location sentence with: "One SQLite file per machine, on node-local disk: `$XDG_CACHE_HOME/toulmin/<project-id>/<hostname>/index.sqlite` when `$XDG_CACHE_HOME` is set and local, otherwise `$TMPDIR/toulmin-$UID/<project-id>/` and then `/tmp/toulmin-$UID/<project-id>/`. `tm doctor` refuses a network filesystem for the index and the stat cache." Append: "On open, the index runs `PRAGMA integrity_check`; a failure or a head that is not on the chain triggers a rebuild."
    *   Line 438 (threat row): Detect or prevent becomes "Detect (recorded head, integrity check) and rebuild"; Remaining risk becomes "A corrupt index that passes the integrity check returns wrong rows until the next rebuild; owner operations never trust it (section 9)".
    *   Line 612 (section 9 intro), append: "Owner operations resolve their targets by event ID and confirm that ID exists on the chain or the named journal before writing; the index is used only to find candidates."

### [Issue 4] design-1-5 [`Design Critic`] - Severity: Major
*   **Affected Scope:** lines 217-218, 415, 812, 1030
*   **Discrepancy Description:** The session socket is under a must-not-mount glob; `separate-user` cannot enter a `0700` directory; no stale-socket rule.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Proposed Edit:***
    *   Line 217 (Network column): "Unix socket at `/tmp/toulmin-$UID/S<n>/sock`; the directory `S<n>/` is the only path under `/tmp/toulmin-$UID/` mounted into the sandbox".
    *   Line 812 (handshake): `"socket":"/tmp/toulmin-1000/S4/sock"`, `"mount_socket_dir":"/tmp/toulmin-1000/S4"`, and must-not-mount keeps `"/tmp/toulmin-1000/*"` with a note line after the block: "`mount_socket_dir` is the one exception to the `/tmp/toulmin-$UID/*` entry; the launcher mounts the directory, never the socket file, so a restarted tool server's new socket is visible."
    *   Line 415 (`separate-user` row), Toulmin's guarantee: "The same, provided the record and lists are owner-readable only and the session directory `S<n>/` is group-accessible to the agent's user and nothing else; `tm doctor` checks modes".
    *   Line 287 (restarting tool server), append: "and removes and recreates its socket file".
    *   Line 1030: "a Unix socket in a `0700` directory" becomes "a Unix socket in a per-session directory under the owner's `0700` `/tmp/toulmin-$UID/`".

### [Issue 5] design-1-6 [`Design Critic`] - Severity: Major
*   **Affected Scope:** lines 290, 328, 363, 403, 618, 647, 1030
*   **Discrepancy Description:** No re-validation of viewer-confirmed actions at read-in; `tm status` silently commits them; one-time code channel unspecified; viewer `OwnerCap` contradicts D27; token path and launch abandonment undefined.
*   ***AI Suggestion:*** PROPOSE FIX (keeps D27; the Zoom Out proposal to drop viewer journals is a separate decision for Stage 8)
*   ***Proposed Edit:***
    *   Line 290, append: "A `viewer.action_confirmed` event records the chain head and the exact target revision IDs at confirmation time. At read-in the Core applies the same checks as `tm apply` (`E8101` stale target, dependency closure, approval gate) plus a check that the launch was still open; a failing action is read in as `viewer.action_rejected` with the reason, never applied. Locked commands other than `tm apply --viewer` and an interactive `tm status` confirmation do not commit pending viewer actions; they list them."
    *   Line 647 (Permission), append: "the one-time code is printed to the terminal running `tm view --serve` and is never sent to the browser".
    *   Line 328 (diagram): change `OwnerCap (confirmation path)` to `per-launch token`; line 1118 (gate A item 43): remove `new_for_viewer_confirmation()`; line 348 wording already changed in Issue 1 (only `tm-cli` constructs).
    *   Line 219 (process table, Writes): "Journal `V<n>`; token file `/tmp/toulmin-$UID/V<n>/token` (0600)". Line 632 (`viewer.launch`): add "`viewer.abandoned` after the session timeout (OQ3) with no `viewer.ended`".
    *   Line 403 (trust table, Owner via viewer): "How Toulmin knows" becomes "Per-launch token plus one-time code at confirmation; re-validated at read-in; events carry `source: viewer`".

### [Issue 6] design-1-7 [`Design Critic`] - Severity: Major
*   **Affected Scope:** lines 119, 220, 392, 831, 1166
*   **Discrepancy Description:** Static view renders agent text with client-side JavaScript and no stated sanitizer or CSP; a browser cannot write `review/actions.json`; `tm apply`'s summary content is unspecified.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Proposed Edit:***
    *   Line 119: replace "which the static view collects into `review/actions.json`; they run `tm apply review/actions.json`" with "which the page's script gathers into an action file the owner downloads (the browser offers `actions.json`; the owner saves it as `review/actions.json`); they run `tm apply review/actions.json`".
    *   Line 220 (process table, Writes): "Pages and (via the owner's browser download) an action file". Line 392 (Browser row, Notes): prepend "Static pages: the same Markdown sanitizer as the viewer, no inline event handlers, no external resources; live pages: ".
    *   Line 831 (11.15), append: "`tm apply` prints one line per item with its target, revision, and any override, refuses the file if any item sets an override and `--yes` was not given per item, and asks for confirmation."

### [Issue 7] design-1-8 [`Design Critic`] - Severity: Major
*   **Affected Scope:** lines 514, 642, 954
*   **Discrepancy Description:** The clearing action "Promote (re-checks against the pinned version)" is impossible: drafts pin fingerprints only, so changed bytes exist nowhere; the keep step fails first.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Proposed Edit:***
    *   Line 514 (clearing action): "`tm claim accept` onto the new version after promotion, or send back; promotion of such a claim is refused with `E6505` evidence changed since drafting".
    *   Line 642 (`draft.promote` library calls): reorder to "Claims: resolve journal events and current revisions; Core: dependency closure and approval gate; Files: confirm each cited version's bytes still match its pinned fingerprint (refuse with `E6505` otherwise); Checker: re-check against pinned versions; Files: keep cited versions on first promotion (Record copies); Claims: record". Errors: add "`E6505` evidence changed since drafting".

### [Issue 8] design-1-9 [`Design Critic`] - Severity: Major
*   **Affected Scope:** line 592
*   **Discrepancy Description:** Panel citations "hold" when the image is missing.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Proposed Edit:*** Line 592: columns "Missing, not kept", "Removable and deleted, not kept", "Fingerprint only (owner chose) and missing" become `not checkable` (the removable one "labeled expected-missing"); keep "Holds (labels are metadata); flag `evidence moved`" only for "On disk, disk differs".

### [Issue 9] design-1-10 [`Design Critic`] - Severity: Major
*   **Affected Scope:** lines 263, 494-495, 871
*   **Discrepancy Description:** The late-output wait and the racy-clean rule as adopted compare clocks across machines; the risk row says never.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Proposed Edit:***
    *   Line 871 (risk row): "Clock skew between nodes mislabels receipts | Run checks compare snapshots with each other, never with a clock. The racy rule compares a file's mtime with the mtime of a marker file the wrapper writes to the same filesystem immediately before each snapshot, so both times come from the same clock. "Run in progress" and "interrupted" use the receipt's own `node.started` plus the recorded walltime request where the scheduler provides one, and are labeled clock-dependent otherwise; `tm doctor` reports skew."
    *   Line 263: "a `started` receipt whose `node.started` plus the late-output wait (setting `run.late_output_wait`, default 10 minutes) has passed by the ingesting node's clock, with no `finished` receipt, shows as "run in progress" and then "interrupted"; this comparison is clock-dependent and is labeled so".
    *   Gate A item 29 (line 1098): add "reference time from a marker file on the same filesystem" (DetailedRange, for consistency).

### [Issue 10] design-1-11 [`Design Critic`] - Severity: Major
*   **Affected Scope:** lines 107-108, 245-247, 624-625, 1020, 1023
*   **Discrepancy Description:** `#SBATCH --signal=B:TERM@120` signals the batch shell only; bash defers traps until the foreground child exits; hashing inside walltime; `tm run finish` on a login node is a long-running process.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Proposed Edit:***
    *   Line 107: "where the script contains `#SBATCH --signal=B:TERM@120` and ends with `exec tm run A3 --param threads=$SLURM_CPUS_PER_TASK`, so the signal reaches `tm run` directly (the user guide also shows the trap-and-wait form for scripts that cannot `exec`)".
    *   Line 108, after "If the job is killed,": "the `SIGTERM` handler forwards the signal to the child, waits up to 30 seconds for it to exit, snapshots the outputs, writes a partial receipt, and exits; `tm run finish R12` completes the hashing later, printing a resubmit line when the outputs are large enough that hashing should run as a job."
    *   Line 625 (`run.finish`): Library calls prepend "print a resubmit line if the recorded hashing estimate exceeds the login-node limit (setting), else"; errors add "`E4205` run on login node, resubmit printed".
    *   Line 1020 (budgets), append: "the user guide recommends padding walltime by the previous run's recorded hashing time".
    *   Gate C exit (line 1225): add "the CUJ 2 scenario test covers the `SIGTERM` path with a real `exec` wrapper".

### [Issue 11] design-1-12 [`Design Critic`] - Severity: Major
*   **Affected Scope:** lines 728-751, 108, 192, 531, 560, 618, 623
*   **Discrepancy Description:** Receipts do not carry the declaration they ran against; behaviour after `tm analysis edit` during a queued job is undefined.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Proposed Edit:***
    *   Line 735 (receipt JSON): after `"analysis": "A3",` add `"declaration": "sha256:<id of the analysis.declared event>",` and inside `"code"` add `"deps": [...], "outs": [...], "excludes": [...]` as the frozen expanded lists.
    *   Line 751, append: "Read-in checks the run against the declaration named in the receipt, not the current one. When they differ, the run is labeled `declaration changed during run`; outputs the current declaration no longer declares are versioned under their (retired) identity with that label, and the run check reports the difference."
    *   Line 505 area (5.6 table): add row "`declaration changed during run` | Label | Run | Receipt's declaration differs from the current one at read-in | Permanent".
    *   Line 560: "A retired file cannot be registered again; it is restored. A receipt may still add a version to a retired identity (labeled `declaration changed during run`)."

### [Issue 12] design-1-13 [`Design Critic`] - Severity: Major
*   **Affected Scope:** lines 575, 634, 979
*   **Discrepancy Description:** Cross-session revision order undefined; race with promotion undefined; ref prefix misleading for attribution.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Proposed Edit:*** Line 575, append: "Revisions of one ref across journals are ordered by read-in order, then journal `seq`; a proposal response is provisional until read-in. A revision read in after the ref has a promoted revision is recorded as rejected with reason "ref already promoted". Every revision event records its authoring session, which the views show beside the ref." Line 979 (D29 Decision cell), append "; order is read-in then seq; late revisions of promoted refs are rejected".

### [Issue 13] design-1-14 [`Design Critic`] - Severity: Major
*   **Affected Scope:** lines 286-290, 298, 58, 618
*   **Discrepancy Description:** Journal fsync before reply unstated; index records length not last ID; no per-journal event-type allowlist.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Proposed Edit:*** Line 286, append: "Every journal line is synced before the writer replies to its caller." Line 298: "each journal's length it was built from" becomes "each journal's last line ID it was built from". Line 726 (11.4): `journal.head_recorded` body already carries `id`; append "Read-in accepts only the event types allowed for the journal's kind (session journals: `claim.proposed`, `claim.revised`, `title.proposed`, `declaration.drafted`, `supersede.proposed`, `accept.proposed`, `note.drafted`, `journal.torn_tail_truncated`; viewer journals: `claim.reviewed`, `run.reviewed`, `viewer.action_confirmed`); any other type is recorded as `journal.event_quarantined` with the offending line's ID and is never interpreted." Line 58 (G5 measure), append: "and the read-in allowlist test shows a forbidden type in a journal is quarantined, not applied".

### [Issue 14] design-1-15 (b-e) [`Design Critic`] - Severity: Major
*   **Affected Scope:** lines 46, 765, 767, 1026, 1028
*   **Discrepancy Description:** Decimal-to-DuckDB mapping, overflow verdict, CPU-dependent code paths, musl stack size, and `encoding` handling are unspecified; line 46's absolute claim is contradicted by line 866.
*   ***AI Suggestion:*** PROPOSE FIX (part (a) escalated as E3)
*   ***Proposed Edit:***
    *   Line 46: "so the same claim gives the same verdict on every machine" becomes "so a verdict does not depend on the host's maths library or DuckDB build (the glibc fallback in section 13 is the stated exception)".
    *   Line 765, append: "Decimal columns map to DuckDB `DECIMAL(38, s)` with `s` fixed per column as the largest scale seen when the parse spec is set; a value that does not fit at that scale is a parse failure (`E3102`), and an arithmetic overflow in `derive` gives verdict `error` with `E5002`. The differential suite includes the same cases run on two CPU generations, since DuckDB selects code paths by CPU feature at runtime."
    *   Line 767 (parse spec): `"encoding":"utf-8"` gets the note "the only value accepted in V1; other encodings are refused with `E3104`".
    *   Line 1026: after "faster allocator", add "and the main thread stack raised to 8 MiB (musl's default is too small for DuckDB's planner)".

### [Issue 15] design-1-16 [`Design Critic`] - Severity: Major
*   **Affected Scope:** lines 448, 453, 497, 627, 967
*   **Discrepancy Description:** Three input rules dropped without a register row: failed run check cancels approval carry-over; reviews record the report file hash; approving with out-of-date inputs warns.
*   ***AI Suggestion:*** PROPOSE FIX (restore (a) and (b); (c) comes from the brief's walkthrough notes and is not in hypothesis v3, so it gets a register line rather than a rule)
*   ***Proposed Edit:*** Line 448 (Approve row, What changes): append "; a rerun whose run check fails does not inherit the approval". Line 497 (`approval required` reason): append ", or a rerun with a failed run check". Line 453 (Review row): append "; a claim review records the registered report's fingerprint at review time". Line 627 (`run.approve`): add warning "`W4302` inputs out of date (warning)" to Errors. Line 967 (D17 Decision cell): append "; a failed run check on a rerun cancels the carry-over".

### [Issue 16] design-1-17 [`Design Critic`] - Severity: Major
*   **Affected Scope:** lines 91, 100, 837, section 5.3
*   **Discrepancy Description:** A committed `.toulmin/project.json` lets a merged agent branch redirect the record path.
*   ***AI Suggestion:*** PROPOSE FIX (OQ6 is a proposed default, so this changes no settled decision)
*   ***Proposed Edit:*** Line 91 (OQ6 default): "`.toulmin/project.json` in the repository root, committed, holding the project ID only; the record path lives in `.toulmin/local.json`, git-ignored, written by `tm init`. `tm` refuses to run when the project IDs in `project.json`, `local.json`, and the record's `record.json` disagree." Add threat row after line 431: "Agent edits `.toulmin/project.json` in its clone to point at a forged record | Session with a clone, merged by the owner | Record path is in git-ignored `local.json`; project ID cross-checked against `record.json` | Prevent | None".

### [Issue 17] design-1-18 [`Design Critic`] - Severity: Major
*   **Affected Scope:** lines 113, 630, 816-821
*   **Discrepancy Description:** `session.start` cannot raise `E7101`; unconfirmed state undefined.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Proposed Edit:*** Line 630: move `E7101` to a new row "`session.confirm` | Owner (launcher calls `tm`) | `OwnerCap`; session open and unconfirmed | Core: record isolation, network mode, mounts hash | `session.confirmed` | `E7101` lists not applied, `E7103` already confirmed". Line 829 (11.14), append: "Until `session.confirmed` is read in, the tool server answers every call with `E7104` session unconfirmed. A session that ends without confirmation is labeled `isolation: unknown` and treated as `unisolated` for every label and warning." Add 5.6 row: "`isolation unknown` | Label | Session and its work | Ended without a launcher confirmation | Permanent".

### [Issue 18] design-1-19 and structural-1-3 [`Design Critic`, `Structural`] - Severity: Major
*   **Affected Scope:** lines 857-874 and the five scattered risk locations
*   **Discrepancy Description:** The risks table omits most failure modes the design introduces; risk material is split across five places with no top-level heading.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Proposed Edit:*** Promote section 13 to a new `## Risks and Mitigations` section placed after `## Alternatives Considered`, with `### Assumptions` and `### Risks` sub-headings, a one-line pointer to 5.3 (threats) and the NFR budgets (limits), and new risk rows: lock holder dies mid-batch or mid-keep (torn tail rule, keep is write-then-verify-then-rename); interleaved appends on a misconfigured mount (fork detected by audit; `tm doctor`); receipt for a changed declaration (Issue 11 label); cross-session revisions (Issue 12 order); clock skew in the racy rule (Issue 9 marker file); index on a network filesystem (Issue 3 doctor refusal); journal durability (Issue 13 fsync); hashing exceeds walltime (Issue 10 resubmit line); viewer-journal read-in (Issue 5 re-validation). Each row cites its section.

### [Issue 19] structural-1-1 [`Structural`] - Severity: Major
*   **Affected Scope:** sections 3, 6, 7, 11, NFR
*   **Discrepancy Description:** Subsection level expressed three ways inside one H2.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Proposed Edit:*** Convert bold pseudo-headings to `####`: in section 3 (`#### 3.1 Main chain`, `3.2 Journals`, `3.3 Receipts`, `3.4 Index`), section 6 (`6.1 Identity level`, `6.2 Version level`, `6.3 Allowed combinations`), section 7 (`7.1 States of a claim`, `7.2 Checks per kind`, `7.3 Evidence-exists outcome table`), section 11 (`#### 11.1` to `#### 11.15`), and NFR (`### Budgets`, `### Portability`, `### Determinism`, `### Security and privacy`, `### Rollout and rollback`).

### [Issue 20] structural-1-2 [`Structural`] - Severity: Major
*   **Affected Scope:** lines 1034-1050
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Proposed Edit:*** Promote `### Future Improvements` to `## Future Improvements` placed after `## Decisions Register` (before NFR), and `### Additional Resources` to `## Additional Resources` placed last before the implementation plan.

### [Issue 21] structural-1-4 [`Structural`] - Severity: Major
*   **Affected Scope:** line 612
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Proposed Edit:*** "Errors are catalog codes; the code scheme is in 11.12, and the codes are listed per row below and collected in the generated `docs/errors.md`."

### [Issue 22] structural-1-5 and comprehension 1 [`Structural`] - Severity: Major
*   **Affected Scope:** lines 945, 951-953, 912, 977; plan rationale items
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Proposed Edit:*** Line 945, append a key paragraph: "Labels used in the Reason and Replaces columns: B1 to B6 are the blockers and M1 to M10 the major findings of the brief's first review (`concepts_brief.md`); "adopt as written" is that brief's list of second-review items folded in without owner judgment; M12 is the sandboxing module of the earlier 15-module spec; "the hypothesis table" and "the modules table" are the writer table in `toulmin_hypothesis.md` decision 1 and the event ownership table in `toulmin_modules.md`." Expand inline at 951-953 ("B3: a leased on-demand service", "B1: an owner socket", "B2: an outside anchor") and 912 ("the earlier spec's sandboxing module, M12"). Fix plan item 44 ("D7 identity rules" becomes "hypothesis decision 7 identity rules; see D5 and D22") and item 50 ("D9 fingerprints item" becomes "hypothesis decision 9, provider checksums").

### [Issue 23] structural-1-6, style-1-2, comprehension 3 and 4 [`Structural`, `Style`] - Severity: Major
*   **Affected Scope:** sections 9, 10, 12; lines 113, 823, 1045
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Proposed Edit:***
    *   One form everywhere: `tm session end S4 --json FILE` (lines 113, 1045); CUJ 3 explains "the launcher writes the end message to a file and calls `tm session end`".
    *   CUJ 3, line 113, after "hands the launcher": insert "(agentbox invokes `tm session start --json`, reads the message, starts the tool server as the owner outside the container by running `tm session serve S4`, writes the bridge configuration with the session token into the sandbox's MCP config, and calls `tm session confirm S4 --json FILE` before starting the agent)". Add `serve` and `confirm` to the `tm session` verbs (line 843) and a `session.serve` row to section 9 ("Owner (launcher) | `OwnerCap` | Core: start the tool server for an open, unconfirmed session | None | `E7105` session not open").
    *   Section 9: add rows for `analysis.edit` (new code version), `analysis.show/list`, `file.versions`, `report.spans`, `report.check`, `doctor`, `lease.status`, `settings.show/unset`, `run.from_draft`, `draft.list/show`, `table.preview` (as read interface aliases where they are reads); mark `viewer.launch`, `viewer.end`, `viewer.confirm_action` as "internal (HTTP endpoints of `tm view --serve`)".
    *   Section 12: add the read-interface subcommands G9 needs: `tm claim citing-rows`, `tm flags list`, `tm status --stale` (for `staleness.list`), `tm status --pending` (for `pending.list`), `tm audit last`, `tm run diff`, `tm session rejections`, and state "`tm table preview` is `files.rows`".
    *   Line 359 (4.3): add "panels set" to the Files events row.

### [Issue 24] style-1-3 and comprehension 2 [`Style`] - Severity: Major
*   **Affected Scope:** glossary and throughout
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Proposed Edit:*** Glossary: split "Record" into "Record directory | The project's permanent store on disk: main chain, journals, inbox, and kept files" and "Record (the) | The owner-accepted content: the main chain and the journal events a promote names"; add "Record library | `tm-record`"; add "Handle | A short readable identifier: `F7`, `R12`, `A3`, `S4`, `V2`, `F7@v2`. Not an ID." Line 96: "Refs:" becomes "Handles used in the journeys:". Replace "file ID", "run ID", "analysis ID", "version ID" with "file handle" etc. in section 10 and CUJ 3, and "project ID" stays (it is a hash). Line 831: `"verdict":"confirmed"` becomes `"outcome":"confirmed"`. Line 189: "Approve | An owner event accepting a code version." Line 360: "approval (acceptance of a code version)". Line 646: "stale action" becomes "outdated action". Line 438: "Index corruption or an index behind the chain". Line 929: "Reports contradictions". Line 163: "One JSON object"; line 177: "A record's" becomes "An event's or object's"; line 898: "Report text stored as Toulmin events".

### [Issue 25] style-1-4, design-1-20 (part), comprehension 13 [`Style`] - Severity: Major
*   **Affected Scope:** lines 184, 483, 502, 531
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Proposed Edit:*** Line 502: "`retired` | Label | File identity | Analysis no longer declares it | Permanent; `tm file restore` adds a `restored` label, and the identity's Retired attribute is computed from the latest of the two". Add row "`restored` | Label | File identity | Owner restored a retired identity | Permanent". Line 531: "Automatic when the analysis stops declaring the output; `tm file restore` records a `restored` label and the attribute becomes no".

### [Issue 26] style-1-5 [`Style`] - Severity: Major
*   **Affected Scope:** throughout
*   ***AI Suggestion:*** PROPOSE FIX (glossary additions plus the mechanical substitutions; "ingest" stays only inside the operation names `run.ingest` and `tm run ingest`)
*   ***Proposed Edit:*** Glossary adds "Fingerprint | A `sha256:` digest of a file's bytes or a directory manifest; the word "hash" is used only for events and IDs", "Lineage | The graph of edges from output versions to the input versions a run used", "Analysis tree | The lineage of one cited version rendered back to registered inputs, with runs, code versions, and approvals", "Proposal | A journal event by a session; "draft" is the set of a session's unpromoted proposals". Substitutions: "ingest(ed) logic/reviews/receipt" in prose to "read in"; "Sweeps are cached ... re-sweeps" to "Disk scans are cached with their age; `tm status --refresh` re-scans"; "query log(s)" to "session diagnostic log(s)" (lines 413, 931, 962, 1030); "out of date" kept as the plain-English form of the staleness state and the glossary says so; "kept-files folder" everywhere.

### [Issue 27] style-1-6 [`Style`] - Severity: Major
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Proposed Edit:*** Add 5.6 row: "`expected-missing` | Label | File version | Marked removable and no longer on disk | Permanent".

### [Issue 28] style-1-7 [`Style`] - Severity: Major
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Proposed Edit:*** Line 120: "`run.approved` events record ..."; "`draft.promoted` events on the main chain point at ...".

---

## Nits

### [Issue 29] design-1-21 - PROPOSE FIX
Line 222: "nothing needs a port" becomes "nothing needs a fixed port; the live viewer binds an ephemeral loopback port". Line 215 (`tm` CLI, Network): "None, except `tm view --serve` (loopback)".

### [Issue 30] design-1-22 - PROPOSE FIX
Line 715: "The line is the canonical envelope with `id` appended as the final key after serialisation (so the line itself is not JCS-canonical); readers strip `id`, recompute, and compare."

### [Issue 31] design-1-23, structural-1-9 (part), comprehension 14 - PROPOSE FIX
Add 5.6 row: "`root not declared` | Label | Lineage edge | The input version has no registered or declared origin | Permanent". G6: "a labeled gap" becomes "a `root not declared` label".

### [Issue 32] design-1-24 - DECLINE
PROV terms are a gate C concern; recorded for the gate C design document.

### [Issue 33] design-1-25 - DECLINE
Recorded. The "container, network off" row is still the right default for local models; the cloud-model warning timing is a gate B wording detail.

### [Issue 34] design-1-26 - PROPOSE FIX
G12 measure: "in CI" becomes "in CI for the three distributions, and on the owner's cluster for the `sbatch` half (recorded as a manual gate C check)". G9 measure: append "the static view embeds each page's source rows as a JSON block for the comparison".

### [Issue 35] design-1-27 - PROPOSE FIX
OQ4 default: append "and 30 seconds of DuckDB time per call (`interrupt` on expiry)".

### [Issue 36] design-1-28 - PROPOSE FIX
Line 585, append: "The "Hashing pending" column applies only to re-checks against a new version that is still hashing; a pending version cannot be cited."

### [Issue 37] design-1-29, comprehension 5 - PROPOSE FIX
Section 3, new paragraph after Index: "**Stat cache.** A node-local file beside the index holding, per tracked path, the last snapshot and the fingerprint it was confirmed against. The wrapper on a compute node does not read a login node's cache: it reads recorded snapshots from the receipts and events in the record directory (a chain scan when no index exists on that node) and writes its own snapshots into the receipt."

### [Issue 38] design-1-30 - PROPOSE FIX
Line 831: `{"op":"claim.review","ref":"S4.12","revision":"sha256:...","outcome":"confirmed","note":"..."}`.

### [Issue 39] design-1-31 - DECLINE
Recorded as an accepted leak; the gate B document should state it.

### [Issue 40] design-1-32 - DECLINE
Recorded for the gate C document.

### [Issue 41] structural-1-7 - DECLINE
Placement of Open Questions is a convention; the forward references are harmless once the section is short.

### [Issue 42] structural-1-8, style-1-10 - PROPOSE FIX (partial)
Remove the duplicated threat-stance sentence at line 75 (Non-Goals) in favour of "See the threat stance in the Overview", and the duplicate no-daemon sentence at line 267. Leave the Alternatives/Register overlap.

### [Issue 43] structural-1-9 - PROPOSE FIX
Line 1004: "Section 4 and the Verification Plan". Line 945: "`settled`" becomes "Settled". D2: "amended by D17, D18, D27" becomes "amended by D18 and D27". Move the gate exit table (lines 1219-1226) under a `### Gate exit criteria` heading inside Future Improvements, leaving a pointer in the Verification Plan.

### [Issue 44] structural-1-10 - PROPOSE FIX
Fence the receipt block as `jsonc`; add one sentence before 11.3: "Schemas written as `{"a","b"}` list keys only."

### [Issue 45] structural-1-11 - DECLINE
Bullet marker style; no reader impact.

### [Issue 46] structural-1-12 - DECLINE
Numbering only the Technical Plan's sections is a deliberate convention.

### [Issue 47] style-1-8 - PROPOSE FIX (partial)
Line 230 and 953: drop "honestly" / "honest wording" for "recorded as `unisolated`" / "wording stated per isolation level". Line 1026: "because musl's allocator is slow under DuckDB" becomes "because DuckDB's own benchmarks show large slowdowns with musl's allocator (measured in the spike)". Leave the rest.

### [Issue 48] style-1-9, comprehension 19 - PROPOSE FIX (partial)
Name the late-output wait as setting `run.late_output_wait` (done in Issue 9). Line 693: "nanoseconds as a separate integer field where sub-second precision is recorded (snapshots and receipts)".

### [Issue 49] style-1-11 - DECLINE

### [Issue 50] style-1-12 - PROPOSE FIX (partial)
Line 1045: "`tm draft promote` and `tm draft send-back`". Line 843: add `--role writer` to the `tm session` notes. Mark all label and flag names in backticks.

### [Issue 51] style-1-13 (and design-1-20 part) - PROPOSE FIX
Remove the `not checkable` row from 5.6; line 13: "reports what it cannot check as "not checkable"".

### [Issue 52] style-1-14 - PROPOSE FIX
Line 557: move "removable" to the Level column: "any, removable | any | any | any".

### [Issue 53] style-1-15 - DECLINE

### [Issue 54] style-1-16 - PROPOSE FIX
Lines 392, 647, 1030, 1047: "Token, Host, and Origin checks"; "Per-launch token, one-time code, Host, and Origin"; "checks Host and Origin" is two items and stays; line 1047 "token, Host, and Origin checks".

### [Issue 55] style-1-17 - DECLINE
Sentence length; the lists in CUJ 3 and 4 would read better split, but the content is unchanged.

### [Issue 56] style-1-18 - PROPOSE FIX
Line 693: "Canonical JSON is RFC 8785 (JCS), implemented in-house and tested against the RFC's examples and a second Python implementation."

---

## Contest

### [Issue 57] design-1-15 claim "D26-D29 'before this draft' versus 'at drafting time'" (preamble remark) - CONTEST
Line 82 says the gaps were "decided with the owner before this draft"; line 945 says "decided at drafting time". Both describe the same event (a decision walkthrough held while drafting, before the text was written). The Zoom Out nit 11 and Comprehension both note the inputs do not contain D26-D29; that is accurate, and the register already marks them "Settled at drafting", which is the evidence a reader needs. No edit; one word change at line 82 ("before this draft" to "at drafting time") is folded into Issue 22 for consistency.

---

# Round 2

**Status: resolved** (defaults taken: E4 worktree mount, E5 `tm-agent` as a third binary, E6 `separate-user` deferred to V1.x; all PROPOSE FIX items applied 2026-11-09; pre-edit copy `.toulmin_v1_design.before_round2.md`). Re-audit cap reached; the 4 declined nits are the only open Stage 7 items.

**Target:** `toulmin_v1_design.md` (FeatureRange, lines 1-1149). Source: `critic_report.md`, round 2. Re-audit cap reached; fixes from this round are applied without a further Stage 7 audit.

Categorisation: 11 Major (3 ESCALATE, 8 PROPOSE FIX), 18 Nit (12 PROPOSE FIX, 6 DECLINE).

## Escalations

### [E4] design-2-3 shared checkout - ESCALATE
The fix that removes the problem is on the agentbox side: mount a `git worktree` of the repository (separate checkout on a session branch, shared `.git`) instead of the working tree itself. Then the owner's HEAD is untouched, owner commands during a session run against the owner's checkout, raw inputs in the owner's tree are not reachable, and the backup copy is unnecessary (the worktree is disposable). The alternative keeps the bind mount and adds a stated protocol: the owner runs no `tm run` or `tm claim add` and edits nothing while a session is open; `tm status` warns when HEAD is on a session branch; the start message carries a `mount_read_only` list (registered inputs, `.toulmin/project.json`). Both variants get the new 5.3 row. Decision needed: (a) worktree mount, or (b) bind mount plus protocol.

### [E5] design-2-1 tool server packaging - ESCALATE
(a) Ship `tm-agent` as a third static binary that does not depend on `tm-cli`; `tm session serve` execs it; CI builds it standalone with the `construct` feature off and greps that only `tm-cli` names `new_for_cli`. (b) Keep it inside `tm`: `tm session serve` and `tm session start --unisolated` take a code path that never constructs `OwnerCap`; CI adds the standalone feature-off build of `tm-agent`; 5.3 and G5 say "compile-time guard" and name the tool surface and mounts as the real controls. Default if no preference: (a), because it is the only version where the process claim ("the tool server holds no capability") is true.

### [E6] design-2-2 `separate-user` - ESCALATE
(a) Specify modes now: a per-session directory `/tmp/toulmin-S<n>-<random>/` created `0710` with the agent user's group, socket `0660`, `tm session start --agent-group GROUP`, `tm doctor` checks. (b) Mark `separate-user` as V1.x beside `separate-host`, keeping its handshake slot. Default if no preference: (b); nothing in gate A or B needs it, and the mode rules are easy to get wrong.

## PROPOSE FIX (Major)

- **design-2-4:** OQ6 gains `tm init --attach RECORD_PATH` (binding only, after checking `record.json`); the binding file records the working-tree roots it was created for, and `tm` refuses an unlisted root (`E2005`) or a missing binding (`E2004`); the 5.3 row's remaining risk becomes "an agent that rewrites `project.json` to another bound project's ID is refused because the working-tree root is not in that project's binding"; `project.init` row lists all three artifacts and `E2004`, `E2005`, `E2006` config directory unwritable.
- **design-2-5:** 3.4: the wrapper builds and reuses a node-local index under `$TMPDIR` with the same fold code; NFR budget row "Wrapper cold start: under 30 s for one million events over a network mount, measured in CI on a generated chain"; Risks row.
- **design-2-6:** `tm session serve` is foreground; the launcher backgrounds and restarts it within a session; `tm status` lists open sessions whose socket is dead, with the restart command; the tool server re-reads `HEAD` and its last `session.*` event on every call and raises `E7104` from that check; `E7104` added to the read-interface and `claim.propose` rows; Setup 3 says `tm session start --unisolated` prints the configuration and the owner runs `tm session serve S<n>` in a second terminal (or `&`).
- **design-2-7:** 3.3: a project-inbox receipt is labeled when any session was open at any instant between the receipt file's ctime in `inbox/new/` and the read-in, plus a grace window (`session.receipt_grace`, default the late-output wait); 3.1: within a batch, read-in precedes the command's own events.
- **structural-2-1:** line 50 points at Future Improvements (Gate exit criteria).
- **style-2-1:** "Record library" in crate contexts; "record directory" in disk contexts; line 193 "into the record".
- **style-2-2:** add `retracted` and `superseded` Label rows to 5.6; line 459 "Items get the `rejected` label".
- **style-2-3:** backticks at every listed location.

## PROPOSE FIX (Nit)
design-N1 (resolved absolute paths in the start message), N2 (align on 3.2), N3 (4.3 rows), N4 (folded into design-2-4), N5 (length check is the protection; doctor probe), N6 (drop `OwnerCap` after `viewer.launch`), N7 (git per node, label); structural-N1 (add codes to rows), N2 (blank lines), N3 (promote glossary sub-heading), N4 (move the schema note to the section 11 intro); style-N4 (word survivors); style-N5 items: heartbeat 5 s setting, split the 70-word sentence, "It" at 307, "never mounted" to "on the must-not-mount list", `retired` cell, `evidence changed since drafting` cell, 7.3 wording, `run.finish` cell, line 114, lines 996, 1008, 1015.

## DECLINE (Nit)
structural-N5 (the rule before NFR; harmless), style-N5 Actor cell length (651), "panel labels" (a figure panel's label is the ordinary English word; the glossary sense is Toulmin's), line 1063 Summary quotation.

---

# Round 3 (Stage 9, DetailedRange lines 1149-1320)

**Status: resolved** (applied 2026-11-09; E7 left to the checklist, E8 option 1 (background hashing deleted), E9 defaults kept; the Stage 8 items 3, 4, and 7 applied; the grace window and binding root list kept. Sections 11.16-11.22 added; the Detailed Implementation Plan replaced with the phased version; `tm lease` renamed `tm lock` throughout. Pre-edit copy `.toulmin_v1_design.before_round3.md`. The document is now 1,548 lines; FeatureRange 1-1359, DetailedRange 1360-1548.)

**Source:** `implementation_readiness_report.md`. **Draft of all proposed content:** `gate_a_plan_draft.md` (Part A: five new section 11 subsections 11.16-11.22; Part B: the replacement Detailed Implementation Plan with phases; Part C: the gate B/C/D items removed per the Stage 8 move list).

Categorisation: 5 Blockers (4 PROPOSE FIX, 1 ESCALATE in part), 22 Majors (19 PROPOSE FIX, 2 ESCALATE, 1 CONTEST), 16 Nits (13 PROPOSE FIX, 3 DECLINE).

## Escalations

### [E7] impl-1-7 reference project and fixture - ESCALATE (fixture drafted; paper is yours)
The draft defines the test fixture (three tables, ten claims, two-page PDF, one PNG) and an `ACCEPTANCE.md` checklist. It cannot name the published paper or where its tables come from. Tell me the paper (or say "leave it to the checklist") and I will fill the gate A exit row.

### [E8] plan-1-10 / Stage 8 finding 2: background hashing - ESCALATE (draft assumes deletion)
No process exists to run it. The draft deletes background hashing: `tm file register` hashes in the foreground; `--defer-hash` records the identity only, and `tm file hash HANDLE` adds the version later; `hashing pending` and `file.hashing_finished` go. This touches the FeatureRange (CUJ 1, 2.1, 5.6, 6.2, 6.3, 7.3, section 9) and is the one Stage 5 finding you never held. Confirm, or say "keep" and I will instead specify a detached child process (the design would then have to allow one process that outlives a command).

### [E9] impl-1-5 dependency choices - ESCALATE (defaults in the draft)
Picked: `bigdecimal` (34 digits; `rust_decimal` caps at 28), `rustix`, `parquet`, `lopdf`, `pulldown-cmark` + `ammonia`, `askama`, `cargo-llvm-cov`, `tempfile`, `assert_cmd`, `libfuzzer-sys`. Exact versions are left to P0 by design. Override any of these.

## PROPOSE FIX (Blockers)
- **impl-1-1 phasing:** the P0-P8 table in Part B, each with a checkpoint command.
- **impl-1-2 event bodies:** 11.16, with the gate A type set enumerated.
- **impl-1-3 index schema:** 11.17, including `head`, `events`, `opaque_events`, `settings`, `claim_cites`, `IndexTx`, and the fold contract.
- **impl-1-4 Checker semantics:** 11.18 (grammar, evaluation rules, assertion rules, description templates); item 93 maps `checker.md` to 11.6-11.10 and 11.18; P4 adds an owner sign-off of `checker.md` before P5.
- **plan-1-1 binaries and CI check:** gate A builds `tm` only; `check-owner-cap.sh` excludes the defining crate and the standalone set is empty in gate A; item 6 reworded.

## PROPOSE FIX (Majors)
impl-1-6 (11.20 page table, review-only action file, `E8102`); impl-1-8 (11.19 settings table); impl-1-9 (11.21 batch write sequence); impl-1-10 (11.21 object store); impl-1-11 (11.22 span grammar); impl-1-12 (item 83 per-command flags, exit codes, gate A verb set; citation syntax in item 19); impl-1-13 (item 43: no `Clone`, `&OwnerCap`); impl-1-14 (Verification Plan rewritten: tolerance file, smoke list once, `tests/scenarios/main.rs`, nightly fuzz job, NFS job, `unittest` not pytest, measurable pass conditions); impl-1-15 (item 41 case list, item 82 test list including parity and holds-scan, G2 invalid cases in the formats suite); impl-1-16 (doctor probes in item 85, logic in a Core op); impl-1-17 (item 47 reader per format, cache writer, Arrow mapping); impl-1-18 (item 80 `audit.json` shape, `--since` rule; item 64 `C<n>` rule); plan-1-4 (item 14 file set matches 11.16); plan-1-5 (`report_check.rs`, `doctor.rs` in Core, gate A verbs); plan-1-8 (NFS case in item 42); plan-1-9 (item 66 subset listed, coverage scoped to gate A codes, `E3103`/`E3104` producers in item 16); plan-1-11 (`stat_cache.rs`, `index/locate.rs`, `project.rs`); plan-1-12 (`build.rs` generator and CI diff); style-1-1 (every D-citation corrected or replaced by a section reference); style-1-4 (`lease` renamed to `lock`); style-1-3 (flag/label/hash/claim wording); style-1-2 (Record directory/library); style-1-5 (noun-phrase item descriptions throughout).

## CONTEST
- **style-1-1, item 44 and 50 "hypothesis decision 7/9":** the citations were added in round 1 to replace wrong D-numbers; the hypothesis is a declared input (line 6) and the Register's key paragraph names it. Kept, with the rule restated inline in the draft so the implementer does not need the input (the restatement also closes impl-N3).

## PROPOSE FIX (Nits)
impl-N1 (formats codes listed in item 20; catalog defined as the union); impl-N2 (gate A list is exactly `CLAUDECODE`); impl-N3 (rules restated); impl-N4 (generator constraint stated; count in item 60); impl-N5 (guide sections listed); plan-N2 (spike Delete; templates and commands enumerated); style-N1, N2, N3 (rationales rewritten); style-N4 (serial "and"); style-N5 (budget checked once); style-N6 (Manual criteria paragraph); style-N7 (asides removed); style-N8 (`draft.rs` not in gate A).

## DECLINE
plan-N1 (three `audit.rs` files: the split is ops/read/orchestration and follows the crate layout); style-N9 ("honours": the document is in the author's spelling); plan-1-16's `E1001` (now listed in item 20, so it resolves).

## Also proposed, outside the DetailedRange
- Stage 8 finding 7 (DVC never weighed): one Alternatives row under "Runs, staleness, and files": "DVC as the run and staleness layer | Declared stages, content fingerprints, carried-forward outputs, dependency-ordered staleness exist | Python runtime and pip install on clusters without admin rights; SQLite state under `.dvc/tmp` on network filesystems; no two-phase receipts from compute nodes; no agent attribution; its own cache duplicates the kept-files folder | Rejected. Toulmin borrows the stage format and reads `dvc.lock` as a V1.x adapter".
- Stage 8 finding 3 (serve before confirm): reorder CUJ 3 and 11.14 so `tm session confirm` precedes `tm session serve`, `serve` refuses an unconfirmed session with `E7105`, and `E7104`, the per-call `HEAD` re-read, and the unconfirmed state are removed. The `isolation unknown` label stays for sessions that end without a confirm event.
- Stage 8 finding 4 (compute-node index): replace 3.4's wrapper-built index and the 30 s budget with the per-analysis `latest.json` the CLI writes at read-in; the wrapper reads one file.
- Stage 8 findings 8 and 9 (grace window; binding root list): the draft keeps both in 11.19 and 11.16/OQ6 because they were applied at your direction in round 2; say "drop" for either and I will remove them.

---

# Round 4 (Stage 9 round-2 findings, DetailedRange and 11.16-11.23)

**Status: resolved** (applied 2026-11-09; D-A = expressions evaluated in Rust over Arrow with `bigdecimal`, relational steps in DuckDB; D-B = the four derived digests renamed `*_hash`). Pre-edit copy `.toulmin_v1_design.before_round4.md`. Document now 1,582 lines; FeatureRange 1-1383, DetailedRange 1384-1582. Stage 9 re-audit cap reached; no further audit.

**Blockers, all PROPOSE FIX, applied:** new shared crate `tm-duckdb` (4.1, 4.4, diagram, item 26a, P2) removes the `tm-files` to `tm-checker` edge; new 11.23 Citation objects with the CLI spelling and escapes, cited from 11.16, item 19, and item 90; `examples/refcheck.rs` and `examples/gencases.rs` in `tm-checker` give the differential script a Rust side before `tm` exists, with two runner classes.

**Majors, all PROPOSE FIX, applied:** phase table (items 1-10 in P0 with the spike-then-delete order; 94 in P1; 72 in P7; 79 only in P8; 82a fixtures in P3); workspace `fixtures/` (item 82a) with the ten claims mapped to every op and assertion kind, `figure.pdf` and `figure.png`, per-claim directories; 11.18.2 "Where" and "Precision" rows (Rust evaluation, 34 significant digits half-even) and `log` is the natural logarithm; 11.18.4 placeholder renderings pinned and `sort.dir` enumerated in 11.8; item 66 split into fold-written and scan-derived flags, `possibly changed (unconfirmed)` added, `retired` moved to gate C; Python `validate.py` for G2; `audit_idempotent.rs` and `claim_pages.rs`; `file lineage`, `table preview --steps`, `table describe`, and the missing flags in item 83; D21 independence rules in item 87 (hand-written `expected.json`, `checker-diffs.md`, `canonical_json.py` import allowed); Coverage row scoped to four crates, an Error codes row with the `TM_ERROR_LOG` mechanism, a Spec drift row and `spec-drift.sh`, `symbol-versions.sh` criteria, both binaries in smoke; 11.17 gains `claim_keys`, `claim_events`, `audits`, `verdicts.body`, `spans.section_kind`, `stat_cache` DDL excluded from rebuilds, enumerated `claims.state`, Core-owned tables named; the fold contract restated (lookup in `events` is the idempotence check); `report.registered` spans carry `inline_code` and `section_kind`; torn-tail producer assigned (item 35 detects and queues, item 75 writes first, `read_in` row lists the event); 11.21 lock-time `HEAD` write, step 0 sealing, 11.3 wording aligned, reflink sentence and dependency removed, heading renamed; `hashing.trust_ctime` added, `harness.env_vars` moved to gate A, `login_node.pattern` to gate C; 8 MiB stack in `tm` (item 5a); `probes.rs` in `tm-fsfacts` (item 30a) with `doctor_report.rs` as renderer only; Rationale citations corrected (items 1, 30, 32, 93); verb forms normalised; `rank.group_by` in 11.8; `--since LAST` defined; `isatty` on stdin; `tm audit --out` writes to `DIR/audit/`; `tm-data` block layout defined; item 10 marked Modify with the document's repository path.

**Nits applied:** Oxford commas in 11.18.2 and item 30a; `exists` fails case; `select` wording; `count` wording; item 9 wording; item 91 given its path; item 15 reference; item 56 wording; item 82 parentheses; 11.19 validation cells; `int` beyond 38 digits is `E3102`; 6.1 "Panel names" on the version.

**Declined:** style-2-13 marker-file phrasing (the two sentences describe the same file from two vantage points and both are accurate).

**Carried from earlier rounds as open, recorded only:** Stage 7 declined nits (4), Stage 9 round-1 declined nits (3), the grace window and binding root list (kept at the owner's direction), the DVC and Snakemake adapters (V1.x).
