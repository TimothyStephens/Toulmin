# Toulmin V1: design hypothesis (version 3)

Nov 9, 2026 · consolidated after two review rounds and two decision walkthroughs

This version replaces versions 1 and 2. It states every decision once. The decision history and reasons are in `concepts_brief.md`. The owner's summary (`Summary.md`) is background for the problem, users, and principles only.

## Hypothesis

Toulmin V1 is a single-user tool, written in Rust, for scientists on Linux workstations and shared clusters. It runs alongside git. It:

- fingerprints the files an analysis reads and writes;
- records declared analysis runs and tells the user which analyses are out of date after an upstream change;
- lets the owner and sandboxed AI agents write reports whose claims cite registered files through declared query steps.

Every claim gets a system check that its cited evidence exists, and quantitative claims also get a check of their assertion. Agents work in journals and draft areas. The owner reviews each claim with the full analysis tree behind it, in a static view or a live viewer, and commits approvals and promotions through the CLI or a confirmation step.

## Threat stance

Toulmin assumes its users are trying to do good work. It:

- makes the right setup the default and easy;
- catches accidental changes, and changes that were not carried downstream;
- makes an agent's shortcuts visible: editing a script until a claim passes, hand-editing a table, or loosening an assertion.

It does not defend against an owner who gives an agent full rights, or against a determined attacker.

## Goals

- **G1. Every claim is linked.** Every claim gets an evidence-exists check, defined for each citation type, and quantitative claims also get an assertion check. Anything the system cannot check is shown as "not checkable", never as passed.
- **G2. Hidden choices become explicit.** Tie rules, missing-value handling, and rank scope are required fields in the query step operations.
- **G3. Reviewers see the evidence and the analysis tree at once.**
  - **Static view:** built early, and a permanent option.
  - **Live viewer:** built last.
  - Both show each claim's evidence and the full tree from the cited file version back to registered inputs.
- **G4. Methods can be reviewed.**
  - **What each run records:** its declared script and support-script fingerprints, the command template and expanded command, the git commit, the diff of declared scripts (including untracked ones), and which commits an agent wrote.
  - **Approval** is required where agents changed the code.
- **G5. Agents do the tedious work safely.**
  - **What agents do:** query, run query steps, draft claims and report text, propose file titles and descriptions, and draft analysis declarations.
  - **Where it goes:** everything lands in the session's journal, attributed to the session.
  - **Owner actions:** agents have no path to them.
  - **Isolation:** guarantees are stated per isolation level, and unisolated sessions are labeled.
- **G6. Provenance goes back to the roots,** as far as declarations allow, with labels for what was declared, carried forward, or changed outside a run.
- **G7. Upstream changes are visible.**
  - **Staleness unit:** an analysis goes out of date when its code, command, or inputs change, or when something upstream does.
  - **Reasons:** each flag gives a plain reason.
  - **Spread:** identical content stops the spread.
- **G8. Files describe themselves** with a stable title, description, and source.
- **G9. Everything is queryable before writing,** through one read interface shared by the CLI, agents, and both views.
- **G10. New versions of cited files are visible.** Quantitative claims are re-checked against the new version, and other claims are flagged. The owner accepts claims onto the new version.
- **G11. Report text and claims stay in sync.** A claim whose sentence in the report differs from its recorded sentence is flagged.
- **G12. Works without special privileges** on a workstation or a shared cluster, including runs inside batch jobs.
- **G13. Validation on request.** `tm audit` re-hashes everything, checks every chain, re-runs every claim check, and reports what changed and why.
- **G14. Modular and replaceable,** with conformance tests for the shared formats, the record, and the checker, and a Python reference checker for differential testing.
- **G15. Imperfect setups still count,** labeled as such.

## Non-goals

- **Judgment.** Judging scientific validity, detecting cherry-picking, or policing writers.
- **Running workflows.** Scheduling jobs, talking to the scheduler, or rerunning chains of analyses. Toulmin wraps the owner's command; `tm run again` reruns one recorded command.
- **Version control and backup.** Git keeps code. Large data is fingerprinted, and only files cited by promoted claims are copied into the record.
- **Deliberate tampering.** Defending against an owner who gives an agent full rights, or against a determined attacker. Hash chains detect accidental damage, isolation keeps agents out of the record, and unisolated work is labeled.
- **Enforced reads.** Enforcing that a run reads only its declared files (V1.x). Staleness is only as complete as the declarations.
- **Sharing.** Signing, collaborators, restriction levels, and export packages (V2).
- **Restricted data.** Handling restricted data for the user. Toulmin provides controls and warnings.

## Decisions

### Process and writers

1. **No central daemon. Every writer owns its own file:**

   | Writer | Writes to | Lock |
   | --- | --- | --- |
   | Owner CLI (`tm`) | The main chain | Short `O_EXCL` lock with an epoch number |
   | Each agent tool server | Its session journal | None; it is the only writer |
   | Viewer | The viewer journal (owner reviews and confirmed actions) | None |
   | Run wrapper (any node) | Receipts in the inbox (two phases: started, finished) | None |

   - **The main chain:**
     - **Fencing.** Before each write, the CLI checks that no higher epoch exists. This stops a paused writer that woke up after a takeover.
     - **Takeover.** It requires an unchanged heartbeat for several minutes, judged by the local monotonic clock.
     - **Reading in.** Every `tm` command reads in new journal lines and receipts, recording each journal's head hash in the main chain.
   - **Journals and receipts:**
     - Each journal has its own hash chain.
     - Receipts arrive by write-then-rename and are read in idempotently.
     - Malformed receipts are quarantined.
     - A fallback inbox inside the project serves compute nodes that cannot see the record directory.
   - **Readers.** The index reads the main chain plus open journals.
2. **Owner actions:**
   - **What they are:** promote, send back, approve, accept claims onto a new version, retract, supersede, update claim text, keep, mark removable, restore a retired file, and settings.
   - **How they are committed:**
     - from the CLI;
     - through `tm apply FILE`, with an action file produced by the static view;
     - through the live viewer's one-time confirmation code plus its per-launch token.
   - **Never** from agent tools or static pages.
   - **In code,** owner operations require an owner capability value that only the CLI and the viewer's confirmation path can create. The constructor lives in a crate the agent tool server does not depend on, which is checked in CI.
3. **agentbox handshake.** These are versioned JSON messages via `tm session start` and `tm session end`.
   - **At start, Toulmin gives the launcher:**
     - a session number (for example S4) and a token;
     - the stdio-to-socket bridge;
     - the agent-denied file list;
     - the must-not-mount list: the record directory, index, stat cache, query logs, viewer token, and other sessions' sockets and journals.
   - **The launcher reports back** its preset, isolation level, and network mode, and confirms it applied the lists.
   - **At end, the launcher reports** the clone path and base and final commits. Toulmin computes `git patch-id --stable` itself.
   - **Crashed launchers.** `tm session close S4 --clone PATH` covers a crashed launcher. A session with no end report is marked abandoned after a timeout.
   - **Trust.** Toulmin trusts the launcher's report.
4. **Agents outside agentbox.**
   - **Labeled sessions.** `tm session start --unisolated --harness NAME` gives a labeled session without agentbox, using the same bridge.
   - **Owner-action labels.**
     - Owner actions record whether they came from an interactive terminal.
     - A known agent-harness environment variable triggers a confirmation and a "possibly agent-issued" label.
   - **Undetectable agents.** An agent run as the owner with none of these signs is not detected.
5. **The agent tool server** runs outside the sandbox, one per session.
   - **Transport:** MCP over a Unix socket under `/tmp/toulmin-$UID/` (path under 107 bytes), reached from inside the sandbox through the stdio bridge.
   - **File references:** tools take registered file IDs only, never host paths.
   - **Refusals** are typed: each names the rule, the bad field, and the owner command that would allow it.
   - **Proposal responses** include the claim's current flags.
   - **Limits.** Per-session limits cap rows, bytes, and calls.
   - **Outside content** is marked as data.

### Files and hashing

6. **File attributes.**
   - **Identity level:**
     - tracked (automatically, as a declared input or output) or registered (given a title and description, and so citable);
     - retired (its analysis no longer declares it; the owner can restore it);
     - removable (the owner marks it).
   - **Version level:**
     - kept (bytes copied into the record when the first promoted claim cites it, or on purpose);
     - hashing pending (not citable until done).
   - **Computed, not stored:** cited.
   - **The design document** gives the allowed combinations.
7. **Identity and versions.**
   - **Identity:**
     - A run output is its analysis plus its declared pattern plus its relative path.
     - An input is its project path. Renaming one needs an explicit command.
   - **Versions:**
     - Different content makes a new version.
     - Identical content (the same bytes, the same table after sorting by key, or the same decompressed content for `.gz` and `.bgz`) is carried forward.
     - Inputs edited by hand become new versions automatically, labeled "changed outside a run".
8. **Declarations** follow the shape of DVC stages: `cmd`, `deps`, `outs`, and `params`.
   - **Paths:**
     - Globs, directories, and list files are expanded and frozen before a run.
     - Exclude patterns and a configurable, recorded default ignore list (`.snakemake/`, `.nextflow/`, `__pycache__/`, `*.swp`, `.DS_Store`) keep noise out of directory manifests.
   - **Command templates** mark which parts may vary.
   - **Support scripts** are declared.
   - **Agent drafts.** Agent-drafted declarations are refused if any path is outside the project.
9. **Hashing.**
   - **Algorithm:** SHA-256 only in V1, with an algorithm prefix on every digest.
   - **Routine checks:** `tm status`, session start, and the viewer use stat only (size, modified time, change time, inode).
     - Small stat-changed files are hashed on the spot.
     - Large ones are listed as "possibly changed (unconfirmed)".
     - Sweeps are cached with their age; `tm status --refresh` re-sweeps.
   - **`tm run`** fully hashes new or stat-changed inputs and outputs, and reuses recorded hashes for unchanged ones.
     - Fresh attributes are forced after the run.
     - Git's racy rule applies.
     - The stat cache is keyed without the device number.
     - Filesystems that do not keep change time reliably are always hashed.
     - Directory manifests are stored once and reused when unchanged, and directory changes report exactly which entries changed.
     - One hashing thread by default inside batch jobs, with a setting for more and a bandwidth cap. Hashing time is recorded.
   - **`tm audit`** fully re-hashes everything on request.
   - **Provider checksums.** A provider's checksum can be checked once at registration.
10. **Tables** have a frozen parse spec:
    - types;
    - missing-value tokens, with `NaN` and `Inf` mapped explicitly;
    - a single or composite key that must be unique.

    A Parquet parse cache is keyed by file fingerprint and parse-spec fingerprint.

### Runs and staleness

11. **Runs execute in the owner's shell,** including inside batch scripts. `tm run` is a Core operation that never takes the lock.
    - **Before the run:** fingerprint the declared scripts, and record the command template and expanded command, git commit, script diff, launch context (batch job, script, scheduler), and scheduler environment variables (`SLURM_`, `PBS_`, `SGE_`, `LSB_`). Snapshot declared inputs and outputs.
    - **Receipts.** The wrapper writes a "started" receipt, then a "finished" receipt with snapshots and hashes. On `SIGTERM` it writes a partial receipt (document `#SBATCH --signal=B:TERM@120`). `tm run finish` completes hashing later.
    - **Labels follow content:** content changed, content unchanged (carried forward), or metadata only.
    - **Inputs changed during the run** are labeled.
    - **Head processes** must wait for their jobs. Bare `sbatch` warns; `sbatch --wait` is accepted. A configurable wait handles late outputs.
    - **Failed or interrupted runs** do not make their outputs current. A declared output that was not produced fails the run check.
    - **Workflow tools:** wrap the head process; it counts as one run in V1.
    - **Job arrays** are one run group with a part per task.
    - **Git calls** use `--no-optional-locks` and only the declared scripts' paths.
12. **Staleness** belongs to an analysis, meaning its latest successful run.
    - **Reasons:** own code changed, own inputs changed, command changed, or upstream changed.
    - **Spread** stops at identical content.
    - **Lineage** is per version, so there are no loops.
    - **Out-of-date analyses** are listed in dependency order with their recorded commands.
    - **`tm run again`:**
      - It reruns one recorded command and warns if the environment differs.
      - For runs recorded inside a batch job, it prints a resubmit line instead of running on the login node.
    - **The software environment** is recorded but is not a trigger.
    - **The Core computes staleness.**
13. **Approval** means "I reviewed the code that produced these outputs, including agent changes, and it is the analysis I intended".
    - **When it is required:**
      - Approval is required for code versions with agent-written changes, or with unattributed changes made while a session was open.
      - Owner-only code is approved automatically, labeled "owner code, not separately reviewed".
      - A setting requires approval everywhere.
    - **Scope:** declared script and support-script fingerprints plus the command template.
    - **Carry-over:** reruns with the same code inherit approval, and a failed run check cancels the carry-over.
    - **Bulk approval** (`tm run approve --pending`) shows each distinct code version once, with agent lines marked.
    - **Promotion gate.** Claims on outputs of unapproved code cannot be promoted without `--allow-unapproved`, which is recorded on the promote event.
14. **Agent attribution.**
    - **On demand.** Agent attribution is computed on demand from session reports: commit SHAs are matched first, then patch IDs.
    - **Partial attribution.** Unmatched commits give "attribution partial", with a count.
    - **Squash merges** that mix agent and owner edits lose attribution for that change.

### Claims and reports

15. **Claims.**
    - **Fields:** sentence, kind (quantitative, qualitative, or mixed), cited file versions (one or more, for joins), citations, query steps, assertion, and qualifiers.
    - **Citation types:**
      - cells, by key column values;
      - whole file;
      - page, checked against the page count;
      - panel, a label registered on the image version.
    - **Reserved for V1.x:** text quote, page region, and image region.
16. **Claim refs.**
    - **Assignment:**
      - Sessions get a short number at `tm session start` (S4).
      - Each tool server numbers its own claims (S4.12).
      - The CLI numbers the owner's claims (C31).
    - **Permanent IDs.** Each ref points to the claim's creation-event ID, which never changes.
    - **Revisions.** A revision keeps its ref as a new event, so assertion changes stay visible.
    - **Supersede** creates a new ref.
17. **Two system checks:** evidence exists, and assertion.
    - **Results:** each is holds, fails, not checkable, or error.
    - **Reviews:** confirmed, disputed, or question. They form a separate line and never override a check.
    - **Outcomes per citation type and file situation** (missing, removable and deleted, fingerprint only, hashing pending) are given in a table in the design document.
18. **Flags.**
    - **One flag per reason** where reasons overlap. "Evidence moved" carries a reason: upstream changed, a newer version exists, or disk differs from the pinned version.
    - **Clearing.** Every flag has a named clearing action, or is a permanent label.
    - **Owner and agent views.** `tm status` and agent queries show each flag's next command.
19. **New versions.**
    - **Re-checks.** Quantitative claims whose steps still run and whose keys are still found are re-checked against the new version.
    - **Flags.** Other claims get "evidence moved: newer version exists".
    - **Accepting.** Claims move to the new version only when the owner accepts them, in bulk. There is no auto-accept.
20. **Reports** are Markdown files in git, registered with `tm report add`.
    - **Section kinds** are heading classes (`## Results {.results}`).
    - **Claims** are spans: `[sentence]{.claim ref=S4.12}`, so one claim can appear more than once.
    - **Drift check.**
      - A small raw-text scanner reads spans from the working tree.
      - Text is compared after normalizing whitespace and Unicode (NFC).
      - Inline code inside a span makes the drift check "not checkable".
      - `tm claim update-text` records an edited sentence and re-runs the checks.
    - **Agents** draft text in their clone, and the owner merges it like code.
    - **Reviews** record the report file's hash.
21. **Promotion, send back, retract, and supersede.**
    - **Promotion** selections must include what they depend on.
    - **Send back** marks items rejected with a note. Later sessions see the rejection.
    - **Retract and supersede** keep the old claim visible.
    - **The owner's own claims** go straight into the main chain.
    - **No automatic conflict detection.** The evidence view shows other claims citing the same rows instead.

### Checker and record

22. **Query steps** are a fixed set of JSON operations owned by Toulmin: filter, select, sort, take, rank, derive, group and aggregate, and join by key.
    - **Versioning:** each operation is versioned on its own.
    - **Required choices:**
      - rank: tie rule and missing-value placement;
      - sort: missing-value placement, with the key appended as the last tie-breaker;
      - filters treat missing values (including NaN) as not matching;
      - `take` requires a total order;
      - group output is ordered by its keys;
      - derive uses a closed expression grammar (no `random()` or `now()`).
    - **The Checker:**
      - **Inputs:** a named set of tables, passed as Arrow, plus the general rules.
      - **Outputs:** per-step outputs and counts, plain-language descriptions, and final rows mapped back to input keys. Aggregated rows give a group key, a member count, and a fingerprint of the member keys.
      - **SQL:** it translates steps into fully explicit DuckDB SQL and stores the SQL for debugging.
    - **Verdict records:** the parse-spec fingerprint, translator version, DuckDB runtime version, loaded extensions, and rule version.
    - **DuckDB settings:** explicit memory and spill settings.
23. **Typed values and assertions.**
    - **Typed values:** one encoding.
    - **Assertions:** one versioned assertion schema with a numeric comparison rule.
    - **Text sort order:** fixed.
24. **The record.**
    - **Files:** a header with the format version, append-only main-chain segments framed by batch length and hash, journals, the inbox, and a compressed, content-addressed folder for kept files.
    - **The index:** a rebuildable SQLite cache, built by each library's own fold from events to tables. It records the chain head it was built from. Rebuilding stays under 60 seconds for one million events.
    - **IDs** are SHA-256 of canonical JSON (RFC 8785, written in-house). Hashed records hold no floats: integers stay within plus or minus 2^53, and everything else is a string. Non-UTF-8 paths are refused.
    - **Integrity.** Every append is synced and close errors are checked. `verify` (part of `tm audit`) reports a torn tail, a fork, and corruption separately. Sealed segments are packed automatically.
    - **Unknown versions.** Unknown event types are read as opaque, and writing is refused.
    - **Reserved slots:** parent run, quote and region citations, signatures and collaborators, and a model-check line.
    - **Errors** come from a catalog of stable codes.
    - **Run events** use W3C PROV terms.
25. **Record scope.**
    - **One record per project.** Its location is set at `tm init`, outside the repository and reachable from compute nodes.
    - **Owner identity:** a random owner ID.
    - **Query logs:** one diagnostic log per agent session, outside the permanent record, storing fingerprints and row keys.

### Review and operations

26. **Views.**
    - **`tm view --out DIR`** writes static pages and is a permanent option.
      - Checkboxes collect approvals, promotions, and reviews into an action file for `tm apply`.
      - The export lists which files its rows came from and honours the agent-denied list.
    - **The live viewer** offers the same views. Owner actions go through a confirmation step, protected by a per-launch token, Host and Origin checks, sanitized Markdown, and a strict content security policy.
    - **Both views** show the analysis tree on every claim page.
27. **Operations commands:** `tm doctor` (filesystem types and mount options, record reachability, clock skew, glibc and kernel, DuckDB version and extensions), `tm lease status`, `tm lease break`, and `tm audit` (including `--since LAST`).

### Build

28. **Rust:**
    - five synchronous libraries, a shared filesystem-facts component, a Core, and three front ends;
    - async only in the agent tool server and the live viewer;
    - Clippy at pedantic level, no `unwrap` in libraries, and `unsafe` only in the DuckDB binding;
    - `cargo-deny` and `cargo-audit`.
29. **DuckDB and the build:**
    - DuckDB pinned exactly, with a committed lock file;
    - extension auto-install and auto-load off, with Parquet compiled in;
    - a fully static musl build with a faster allocator, falling back to glibc 2.17 with static `libstdc++`;
    - CI checks symbol versions and smoke-tests on Rocky 8 and 9 and CentOS 7;
    - the first task is a musl spike.
30. **Testing:**
    - **Conformance cases:** shared formats, record, and checker.
    - **Checker:** property and metamorphic tests, differential tests against a simple Python reference checker written from the spec alone by a separate agent session, snapshot tests of generated SQL, and parser fuzzing.
    - **Python cross-check:** a second Python implementation of canonical JSON and IDs.
    - **Scenario tests:** written so the owner can read them.
31. **Build gates:**
    - **A:** shared formats, Record, Core and CLI, Files (inputs), Checker and its test suite, Claims and report spans, static view, and `tm audit`.
    - **B:** agent tool server, bridge, handshake and unisolated sessions, journals, refs, promote, send back, and `tm apply`.
    - **C:** Runs, staleness, new versions, approval, attribution, run groups, and drafted declarations.
    - **D:** the live viewer.

    The first real test re-derives the claims of one of the owner's published papers from its registered tables.

## Deferred

- **V1.x:**
  - PDF and web text extraction, and quote checks;
  - agent-registered inputs with an upload tool;
  - export and import;
  - field profiles (RNA-seq first);
  - number detection in claim text;
  - runs enforced in a container or traced;
  - Snakemake and Nextflow adapters for per-rule lineage;
  - format-aware comparison (BAM `@PG`, PDF dates);
  - a text front end for query steps;
  - agents on a separate host (an SSH-forwarded socket plus a token);
  - a faster hash algorithm, if measured to be needed.
- **V2:** signing, collaborators, restriction levels, and the RO-Crate packaging wrapper.
- **Later:** model checks, labs, and an open format standard.

## Terms (seed for the glossary)

| Term | Meaning |
| --- | --- |
| Analysis | A declared recipe: command template, scripts, inputs, and outputs |
| Run | One execution of an analysis |
| Query steps | The JSON operations a claim applies to a table |
| Main chain | The owner's append-only event history, written only by the CLI |
| Journal | One writer's append-only file: an agent session, or the viewer |
| Segment | One file of the main chain |
| Agent-denied files | Files the agent tool server refuses to serve |
| Accept | Move claims onto a new file version (owner only) |
| Restore | Bring a retired file back |
| Flag | System-raised attention with a clearing action |
| Label | A permanent attribute |
