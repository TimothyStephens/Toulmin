# Toulmin V1 core design

* **Author:** Tim (Timothy Stephens)
* **Date:** 2026-11-09
* **Status:** Final brainstorm brief, after two rounds of review by six independent reviewers (architecture, scope, technical accuracy, newcomer reading, build readiness, and document structure) and two decision walkthroughs with the owner. All 25 decisions (D1 to D25) are closed.
* **Authoritative design:** `toulmin_hypothesis.md` (version 3) and `toulmin_modules.md` (version 3) consolidate every decision into one place. The decision lists in this brief are the history of how each was reached; where an earlier item conflicts with a later one, the later one wins, and version 3 already reflects that.
* **Background only:** the owner's `Summary.md` (Nov 9), and the earlier 15-module spec as a record of past decisions.

## Overview

Toulmin links every claim in a scientific report to the exact evidence behind it: a registered file, the declared steps applied to it, the rows or region it cites, and the history of how that file was made. The system checks what can be checked, and people review the rest. It is built first for reports drafted by AI agents and reviewed by the scientist who owns the project.

V1 is for one scientist on Linux, working alone or with sandboxed agents, on a workstation or a shared cluster without admin rights. It runs alongside git. In V1, Toulmin:

- fingerprints files;
- records declared analysis runs;
- lets the owner and agents write reports whose claims cite registered files;
- checks every claim's cited evidence still exists, and checks the assertion of every quantitative claim;
- lets the owner review and promote agent drafts.

agentbox, a separate project, launches and isolates agents. Toulmin records which isolation each agent session had.

The first review supported the overall shape but found that the trust model and process design did not hold for the stated targets. The owner then set a threat stance (help people doing good work; catch accidents and agent shortcuts; do not defend against determined attackers), and the design was rebuilt around it. The second review confirmed the rebuilt shape and found gaps in the no-daemon writer model, IDs, hashing cost, and staleness noise. All of these were closed in the second walkthrough.

## Analysis

A finding marked (n/6) was raised independently by n of the six reviewers.

### What held up

- **The checker as a pure function**, with no access to the record or the disk, is the right boundary. It is also the one place where formal conformance tests clearly pay off.
- **Moving sandboxing to agentbox** and keeping only a handshake is right, provided the handshake carries more data (see B1 and M1).
- **Declared runs, labeled honestly**, fit the goal that imperfect setups still count.
- **A single writer for the record** is right for a store kept on network filesystems.
- **The cuts made during clarification held**: the fuzzy image check, agent test runs, the exploration count, signing, export and import, and PDF quote checks.

### Blockers: resolve before drafting

#### B1. "Owner-only by connection" holds only for a Unix socket the agent cannot reach (6/6)

The rule breaks in several common setups, and in each one the session is still labeled as isolated:

- **Shared loopback.** On a shared login node, 127.0.0.1 is shared by every user. A local viewer that accepts promotions, or the local end of an SSH tunnel, is open to every other user on the node.
- **Browser attacks.** A local web viewer can be attacked through the owner's browser: cross-site request forgery, DNS rebinding, or links and HTML in agent-written Markdown.
- **Containers reaching the host.** Rootless podman containers run under the owner's own user ID, so checking the user ID alone cannot tell owner from agent. A container on host networking, or one whose network mode maps the host's loopback, can reach the owner's ports.
- **Credentials inside the sandbox.** A forwarded `SSH_AUTH_SOCK`, SSH keys, or Kerberos tickets inside the sandbox defeat "separate machine" and "container" alike.
- **One-way handshake.** The launcher reports to Toulmin, but Toulmin cannot tell the launcher which paths must never be mounted.

Direction:
- **Owner socket.** Accept owner actions only over a Unix socket in a node-local directory with mode 0700. For every connection, read the caller's process from the kernel (`SO_PEERCRED`) and compare its user and mount namespaces with the service's. Refuse callers from any other namespace. Mount the socket's directory into sandboxes, never the socket file, so a restart doesn't leave a stale socket.
- **Viewer.** For V1, the viewer is for reading and entering reviews only. Promotion and run approval stay in the CLI, as in the earlier spec. Viewer writes need a per-launch token, Host and Origin checks, CSRF tokens, sanitized Markdown, and a strict content security policy (Jupyter's model).
- **Two-way handshake.** Toulmin sends a must-not-mount list: the record directory, the owner socket directory, token files, and files kept away from agents. The launcher returns its network mode and a hash of its actual mount list. Optionally, the launcher probes the owner socket from inside the sandbox and records the result.
- **Separate host.** Forward a Unix socket over SSH (`ssh -R` with a socket path) and require a session token on every request. The agent host must hold no credentials for the service host.

#### B2. Without signing or an outside anchor, the record detects only accidental damage (6/6)

With no isolation, an agent running as the owner can rewrite the whole record and recompute every hash, and `verify` still passes. The non-goal ("detected and reported, not prevented") and the summary's "always tamper-evident" overstate V1. Signing in V2 does not fix this alone: an unisolated agent can read `~/.ssh` too.

Direction: do both of these:
- **An outside anchor.** At each promotion and approval, write the chain's latest hash somewhere the agent cannot quietly rewrite. One option is a git ref or note that the owner pushes to a remote the agent has no credentials for. `verify` compares against the latest anchor.
- **Honest wording.** "Detects corruption and partial edits. Detects a full rewrite only against an outside anchor. During an unisolated session, who did what is not reliable." Label all work done during an unisolated session that way.

#### B3. A long-lived service does not fit clusters (5/6)

Five problems, all on the cluster:
- Login nodes often kill long-running user processes.
- Users land on different login nodes from one session to the next.
- Unix sockets do not cross nodes, so a `tm run` inside a batch job on a compute node cannot reach a service on the login node.
- A service that runs commands itself gets the wrong environment: no `module load`, no conda environment, no SLURM variables.
- `tm run -- sbatch job.sh` returns in a second, before any output exists.

Hashing terabytes over NFS, or loading a multi-GB table, inside the one process also blocks everyone else and risks a memory kill.

Direction: separate "one writer" (a rule) from "a daemon" (one way to host it).
- **Lease.** Exactly one writer per record, enforced by a lease file holding host, process ID, and heartbeat. Do not rely on `flock`: Lustre often uses node-local locks, and NFS locking varies.
- **On-demand service.** When no service is running, the owner's CLI hosts the writer in its own process. A service starts on demand for agent sessions and the viewer, and exits when idle.
- **Runs.** A small wrapper runs the command in the owner's own shell, where the data is. It snapshots and fingerprints files and writes a self-contained run receipt. The writer takes in receipts whenever it can, re-checking fingerprints. A run never needs to reach the service.
- **Blocking runs.** V1 requires a head process that waits for its jobs. Run `tm run` inside the batch script, and warn on `sbatch` without `--wait`. Add a configurable wait for outputs that appear late on shared filesystems.
- **Heavy work.** Hashing, table loading, and bulk re-checks run in worker processes with memory limits. Owner requests go ahead of agent requests. Per-session limits on rows, bytes, and calls are restored.

The scope reviewer goes further: no daemon at all. Instead, a library with three entry points (CLI, viewer, and one tool server per agent session, started by the launcher). This differs from the direction above only in whether agent sessions share one process.

#### B4. The run check fails on normal Snakemake use (4/6)

"Every declared output was created or changed during the run" breaks in practice:
- **Incremental reruns.** Snakemake and make skip outputs that are already up to date, so the check fails on outputs that are legitimately current.
- **Wrong passes.** `--touch` and `touch` change a file's time without computing anything.
- **Wrong failures.** `cp -p`, `tar`, `gunzip`, and `rsync -a` keep old modified times.
- **Unreliable clocks.** NFS caches file attributes, clocks differ between nodes, and timestamps are coarse.
- **Late fingerprints.** The script and inputs are fingerprinted after the run, so an edit during a long run is recorded as if it had been used.

Direction:
- **Before the run:** fingerprint the script and record the git commit and whether the working tree has uncommitted changes.
- **Snapshots.** Snapshot declared inputs and outputs before and after the run (device, inode, size, modified time, change time). Compare the snapshots with each other, never with a clock. A file counts as written by the run if it is new, or if its inode or change time changed. Users cannot set change time.
- **Labels instead of failures:**
  - An output not rewritten, whose fingerprint matches an earlier version, is labeled "carried forward".
  - An input changed during the run is labeled.
- **Explainable failures:** store the snapshots in the check result.
- **Cheap rechecks:** keep a git-style stat cache, and re-hash any file whose modified time is at or after the snapshot time.
- **Big workflows:** allow globs, directories, and list files in declarations, since a genomics workflow can have hundreds of paths.

#### B5. The file lifecycle contradicts itself (5/6)

The documents use about nine words for file states and give three answers to "what makes a file citable" (registered, flagged, or cited). Two consequences:
- **Unreviewed copies.** Agent flags and draft citations force permanent copies into the record before the owner reviews anything. One session flagging BAM or h5ad files could fill a 20 to 50 GB home quota on a cluster.
- **Dependency cycle.** "Cited, so never removable" needs knowledge of claims inside the files module.

Direction:
- **One lifecycle table**, owned by the files module. For each state: how a file enters it, who may trigger that, required metadata, whether bytes are copied, whether it is citable, and whether agents can see it.
- **Recommended citability rule:**
  - Registration makes a file citable, and its fingerprint is pinned.
  - Bytes are copied when a *promoted* claim first cites the file.
  - Agent drafts pin fingerprints only.
  - The files module holds a neutral "pinned" flag that the service sets; it never asks the claims module.
- **Storage:**
  - A size warning before any copy, and a byte budget per project.
  - A configurable record location, with a warning when a file sits on a scratch filesystem that purges old files.
  - Compressed copies, and reflink copies where the filesystem supports them.
- **Service-level views.** Retiring files and counting cited versus uncited files become compositions in the service.

#### B6. Shared formats and module ownership are too open to start module 0 (3/6, but blocks the build)

Undecided:
- **Formats:** serialization and IDs; the citation formats (composite keys, page, panel, whole file, plus room for quotes); the enumerations; the V1 owner identity (V1 has no keys).
- **Record:** whether there is one record per project or per user, and where it lives (outside anything agents mount); how drafts are stored and what promotion does to them; all-or-nothing batch writes.
- **Ownership:** which module owns which record type, and which modules may call which.
- **Language:** the implementation language, which is never stated.

The earlier spec settled most of these, and the new module list dropped them without replacing them.

Direction:
- **Library pattern.** Modules 3 to 7 are libraries. Each gets a read-only view of the record and returns proposed records. Only the writer appends them, in one all-or-nothing batch. Modules never call each other: the service passes data between them, including profile rules into the checker.
- **A decision sheet for module 0**, covering the items above.
- **Room for later:** a random owner ID created at setup, which a V2 key can later bind to; reserved values for collaborators, signatures, quote citations, and model checks.

### Major findings

#### M1. Nothing tells Toulmin which code an agent wrote (5/6)

Runs must mark script changes made by agents. The earlier spec recorded this when the owner fetched an agent's branch, and that step left with the sandbox module. Commit trailers and author names are written by the agent and are easy to forge.

Direction:
- **Per run:** record the git commit and whether the working tree has uncommitted changes. The commit covers every script in the repository, which may replace hand-declared support scripts.
- **Per session:** at session end, the launcher reports the clone's base commit, head commit, and each commit's `git patch-id`. Patch IDs survive rebases, so Runs can mark matching commits as agent-written.

#### M2. What happens on a new file version is undefined (6/6)

- **Two readings of "re-checked automatically".** The hypothesis can be read as moving a passing claim to the new version, which conflicts with the summary's "the owner decides, auto-accept is an opt-in".
- **No file identity.** File identity across versions is not defined.
- **Constant flags.** Plotting libraries embed creation dates, so every rerun makes a new fingerprint and would flag every qualitative and image claim.

Direction:
- **Identity:** analysis plus declared output name.
- **Re-checks** write a system verdict against the new version. The claim keeps citing the old version until the owner accepts it, in bulk.
- **Unchanged content:** identical bytes, or identical parsed content sorted by key, is carried forward without a flag. Recommend `SOURCE_DATE_EPOCH` for figures.
- **No auto-accept opt-in in V1.**

#### M3. Keys and reproducible verdicts need more than the boundary carries (4/6)

Keys repeat (gene symbols), need composites (gene by contrast), and drift between versions (Ensembl version suffixes, `"007"` versus `7`). Steps that aggregate or join produce rows with no input key. CSV type guessing, null sort order, and parallel float sums all vary between engine versions.

Direction:
- **Frozen parse spec:** types, missing-value tokens, and a single or composite key. Consider Frictionless Table Schema or CSV on the Web rather than a new format.
- **Keys:** check uniqueness at registration, match keys exactly, and have the steps output map each final row back to its input keys.
- **Recorded versions:** each verdict records the parse spec fingerprint and the parser, translator, engine, and profile versions.
- **Deterministic settings:** one thread and a stable sort.
- **Debugging aid:** store the generated SQL with each verdict, not as the authority.

#### M4. A step-at-a-time query language already exists (2/6)

PRQL is open source, compiles to DuckDB SQL, and reads like the declared steps (`from de | filter padj < 0.05 | sort {-log2FoldChange} | take 10`). To use it:
- store the PRQL text as the declared steps;
- run each prefix of the pipeline to show each step's output;
- check required choices (ties, missing values) on its syntax tree.

The fallback is a JSON list of six to eight operations rather than a new language. Ibis and Substrait are other options.

#### M5. "Evidence exists" overstates what is checked (4/6)

- **Panels:** a free-text panel label cannot be checked from image bytes.
- **Whole files:** a copied whole file always passes.
- **Pages:** checking a page needs the PDF's page count, which the files module does not supply.

Direction:
- Define what the check compares for each citation type.
- Make panel labels registered metadata on each image version.
- Report anything else as "not checkable", never "passed".

#### M6. The owner's own workflow has gaps (2/6, high impact)

- **No correction path.** Records are never overwritten and editing is deferred, so a typo in a promoted claim has no fix. V1 needs an owner-only retract or supersede action.
- **Owner acts are undefined:**
  - **Promote, review, approve, and "send back"** are not told apart: what each changes, whether promotion can be partial, and who receives "send back" once the session has ended.
  - **Run approval** has no stated effect. It is unclear what approval enables, or whether it is a label only.
- **Where report text lives:**
  - **Location:** in the record, or as Markdown files in git. The scope reviewer recommends files in git, with Pandoc spans such as `[sentence]{.claim #C12}` and section classes. Agents would draft text in their own clone, and claims would keep the draft area.
  - **Human writers:** the path for a person writing claims is unstated.
- **Owner bottleneck:**
  - Each analysis the owner runs means retyping the agent's work by hand. An agent tool that drafts an analysis declaration (script, inputs, outputs) would let the owner review, run, and approve it in one command.
  - Agents still never run real analyses themselves.

#### M7. Permissions, opt-ins, and labels are scattered and incomplete (3/6)

**Owner actions not assigned to anyone:**
- marking a file removable;
- the setting that keeps a file away from agents;
- accepting a new version;
- clearing a flag;
- retiring or restoring a file;
- choosing a table's key column;
- running with no isolation.

**The agent-denied setting** only covers what Toulmin serves unless the launcher receives the list (B1).

Direction: three single tables (owner-only actions, opt-ins, and labels), stated once and referred to everywhere else.

#### M8. Agent-facing details

- **Registered IDs only.** Agent tools must take registered file IDs, never host paths. A symlink in the agent's scratch area could otherwise get the owner's SSH key copied into the record.
- **Mitigation, not guarantee.** Labeling outside content as data reduces prompt-injection risk. The real control is that agents cannot perform owner actions.
- **The query log** stays outside the permanent chain, stores result fingerprints and row keys rather than data, and has a stated retention period.
- **The agent interface** should name MCP as its protocol. Its tool schemas then double as the guide agents need to write valid claims.

#### M9. Scope cuts proposed by the scope reviewer

- **Profiles:** fold the general rules (state ties for ranks, state null handling for filters and sorts) into the checker's validation. Split profiles out when RNA-seq arrives.
- **Conflict detection:** replace with "other claims citing these rows" in the evidence view, and leave the judgment to the reviewer.
- **Agent-registered inputs:** refuse in V1 with no opt-in. No agent tool for it exists anyway.
- **Fewer parts:**
  - About five parts instead of nine: record and files; runs; checker; claims; and one library with CLI, agent, and viewer front ends.
  - Formal conformance tests only for the checker and the record format.
- **`verify --recheck`:** re-runs every check from the record and compares the results with stored verdicts. This keeps the reproducibility test that export and import used to provide.
- **Record storage:**
  - one SQLite event table, where each row carries the previous row's hash, plus a content-addressed folder for copies;
  - drafts as a column, not as branches;
  - not one file per record (inode limits on clusters).

#### M10. Gaps in the design documents (structure, newcomer, readiness)

- **A decisions register**, with a status and reason for each decision, replacing "where they disagree, the hypothesis wins". The summary is out of date in at least eight places, including export, PDF evidence, the fuzzy check, the RNA-seq profile, signing, the build order, and Goal 9.
- **A glossary**, including the mapping from Toulmin's terms (grounds, warrant, backing, qualifier, rebuttal) to system terms, and what each means for qualitative and image claims.
- **One meaning per word.** "Flag", "check", "step", "input", "declared", and "version" each carry three to five meanings now. Suggested:
  - "query steps" for claim steps;
  - "analysis" and "run";
  - "mark important" or "mark removable";
  - "flag" only for system-raised attention;
  - "label" for permanent attributes;
  - distinct names for the run check, claim check, disk scan, and record verify.
- **Testable success measures** tied to goals:
  - the same verdicts on two machines;
  - every claim in a reference project re-checks after a version change;
  - no owner action is possible from an isolated session;
  - `verify` catches each listed tampering case.
- **Assumptions and risks:**
  - git is present;
  - agentbox implements its side of the handshake;
  - tables have unique keys;
  - git does not back up large data files.
- **A threat table** for the trust model, with columns: threat, attacker position, V1 control, detect or prevent, and remaining risk.
- **A process and deployment section**, and a boundary table that includes external systems (git, agentbox, the browser, the filesystem, cloud model providers).

### Nits

- The modules file says "seven working parts" but lists nine V1 modules.
- Directory outputs (BLAST databases, Parquet folders) need a defined fingerprint, for example a hash over sorted entries like a git tree, or BagIt manifests.
- Unix socket paths are limited to about 108 bytes, and `$XDG_RUNTIME_DIR` often does not exist on clusters. Use `/tmp/toulmin-$UID/` and check its owner and mode.
- Clients and the service exchange version numbers. Session token hashes persist across a service restart.
- Retiring a file should be reversible by the owner.
- Refusals name the rule, the item, and the owner command that would allow it. Agent errors point to the bad field.
- Reword the non-goal as "Toulmin wraps your command; it does not schedule or replace your workflow tool."
- Use W3C PROV terms (used, generated by, associated with) in run records, so a later RO-Crate export is a mapping, not a migration. in-toto link metadata is prior art for run records with input and output hashes.

## Second review of the version 2 design (Nov 9)

Six fresh reviewers read `toulmin_hypothesis.md` and `toulmin_modules.md` (both version 2), using this brief as the record of earlier findings.

### What held up

- **First-round blockers are closed under the threat stance:**
  - **B1, the owner path:** owner actions are CLI only.
  - **B2:** honest wording about tampering.
  - **B4, the run check:** snapshot comparison.
  - **B5, file lifecycle:** bytes are copied only on promotion.
  - **B6, the library pattern:** decided.
- **Decisions confirmed:**
  - The Checker as a pure library, libraries that never call each other, and the Core as the only writer.
  - Segment files rather than SQLite as the record, since SQLite is unsafe on network filesystems.
  - PRQL rejected.
  - Staleness and text drift kept.
  - A minimal agentbox handshake.

### New problems created by the version 2 decisions (converging findings)

- **Too many writers for one lease (6/6).** The viewer and every agent tool server write events too.
  - A writer holding the lease for its whole life blocks the CLI.
  - Taking the lease per write creates thousands of segment files.
  - Takeover compares clocks across hosts, and nothing stops a paused writer from waking up and forking the chain.
- **Claim IDs conflict with report spans (4/6).** Content-hash IDs change when a claim changes. Nothing allocates `C12` without a central process, and two agent sessions can both write `C12`.
- **The run wrapper breaks the module rules (5/6).**
  - It sits in Runs but is called by the CLI.
  - It hashes files, which duplicates Files.
  - Background hashing has no process to run it.
- **Hashing cost on clusters (2/6, high impact).** "A full hash on every run output" re-hashes a whole 500 GB results folder after a one-file rerun. Low I/O priority has no effect on NFS, Lustre, or GPFS.
- **Agent harnesses cannot speak MCP over a Unix socket (3/6).** A stdio bridge inside the sandbox is needed.
- **Cross-machine verdicts can differ (1/6, technical).**
  - The C maths library (log, exp) differs between older and newer cluster systems.
  - A tilde pin lets DuckDB patch releases change floating-point sums.
  - DuckDB can auto-load extensions.
  - DuckDB treats NaN as larger than every number, so "missing values do not match" fails for `NaN` written by pandas.
- **Staleness noise (3/6).**
  - Compressed outputs, BAM headers, and plots differ on every rerun, so the "identical output" cutoff rarely fires.
  - Expanded commands include run-specific values such as CPU counts.
  - Edits to config files each need an accept step.
- **Too many claim flags (2/6).** One claim can show about eight markers. Four of them mean "your evidence moved", and most have no clearing action.
- **Run approval friction (3/6).** Every rerun, and every job-array task, needs a new approval. Approval also duplicates run review.
- **Missing pieces V1 needs:**
  - globs, directories, and list files in declarations (dropped by accident);
  - joins citing two tables;
  - two-phase run receipts for jobs killed at their time limit;
  - agents run outside agentbox, the likely default on clusters, bypassing drafts entirely.

### Decisions for the owner

| # | Decision | Recommendation |
| --- | --- | --- |
| D15 | Writer model | Each agent tool server, the viewer, and the run wrapper append to their own journal: one writer per file, with its own hash chain and no lease. Only the CLI writes the main chain, under a short `O_EXCL` lease with an epoch number that fences paused writers. The index reads the main chain plus open journals. Promotion points at journal events |
| D16 | Claim IDs in report spans | A claim's ID is the ID of its creation event and never changes. Spans carry a short hash prefix as an attribute (`[sentence]{.claim ref=c3f9a2}`), extended on collision. Display aliases (C12) are assigned at promotion and never written into files |
| D17 | Run approval | Approval is a confirmed review of a run, scoped to its code. It carries over to reruns with identical script fingerprints and command template. Bulk approval of a listed set is allowed. Promotion still refuses claims on unreviewed runs unless the override is given |
| D18 | Viewer | Keep the live viewer in V1 but build it last. Add `tm view --out DIR` (static pages) early, so evidence can be read before the live viewer exists |
| D19 | Hashing run outputs | Full-hash only entries whose before-and-after snapshots differ. Reuse earlier hashes for unchanged entries. Store directory manifests de-duplicated. Replace the I/O-priority promise with a bandwidth cap. This revises "full hash on every run output" |
| D20 | Build gates | Gate A: Record, Core, CLI, Files, Checker, and Claims on registered inputs, plus static view. Gate B: agents drafting on registered inputs. Gate C: runs, staleness, versions, and attribution. Gate D: live viewer. Agents are tested before the cluster machinery |
| D21 | Python reference checker | Keep it, built after the conformance cases, written by a separate agent session from the spec alone. Its main use is generating random cases. Allow last-digit differences for log and exp |
| D22 | Staleness meaning | Staleness belongs to an analysis (its latest run), with plain reasons: "own code or inputs changed" or "upstream changed". Compressed files are compared by their decompressed content for the cutoff. Commands are declared as templates, with marked parts allowed to vary. Staleness is computed from what is on disk, and accept-or-restore applies only to registered inputs that nothing has used yet |
| D23 | Agents outside agentbox | `tm session start --unisolated` gives them a labeled session and a draft area. The CLI labels owner actions "possibly agent-issued" and asks for confirmation when it sees a known agent-harness environment variable. This is visibility, not prevention |
| D24 | DuckDB and build | Pin DuckDB exactly, with a committed lock file. Turn off extension auto-install and auto-load. Build fully static with musl (shipping its own maths library and a faster allocator), with a glibc 2.17 build as the fallback. Smoke-test on RHEL 7 and 8. This revises the tilde pin |
| D25 | Files and Runs boundary | Stat snapshots, hashing, and directory hashing move into one shared component with no state. The run wrapper becomes a Core operation that never takes the lease. The Core owns staleness |

### Adopt as written (no owner judgment needed; folded into the spec)

- **Record:**
  - Head file with sequence, segment, offset, last hash, and epoch.
  - Batches framed with a length and a hash.
  - `verify` reports a torn tail, a fork, and corruption separately.
  - Every append is synced, and close errors are checked.
  - Sealed segments are packed automatically.
  - An index rebuild budget of under 60 s for 1M events.
  - Each library supplies its own fold from events to index tables.
  - The index records the chain head it was built from.
- **Inbox:**
  - Maildir-style write-then-rename, and idempotent ingest keyed by the receipt hash.
  - A quarantine folder for malformed receipts.
  - A fallback inbox inside the project, plus `tm run ingest PATH`.
- **Receipts:**
  - Two phases: started, then finished.
  - The wrapper catches `SIGTERM` and writes a partial receipt. Document `#SBATCH --signal=B:TERM@120`.
  - Status shows "run in progress" and "interrupted".
- **Stat cache:**
  - Keyed without the device number.
  - Git's racy-clean rule.
  - Fresh attributes after a run (`statx` force-sync, or open before stat).
  - Hash on filesystems that do not keep change time reliably.
  - Recorded stat values make cache entries derivable on any node.
- **Labels follow content:** content changed, content unchanged, or metadata only. Stat data only decides when to hash.
- **Numbers in hashed records:** integers within plus or minus 2^53, and everything else as strings (decimal strings, second and nanosecond pairs, inode numbers). The in-house canonical JSON is then small. Non-UTF-8 paths are refused with a clear error.
- **Typed values, assertions, and sorting:** one typed-value encoding, one versioned assertion schema with a numeric comparison rule, and fixed text sort order.
- **Missing values:** parse specs map `NaN` and `Inf` explicitly. The translator guards against NaN. `take` requires a total order. Group output is ordered by its keys. Derive uses a closed expression grammar (no `random()` or `now()`).
- **Checker input and output:**
  - It takes a named set of tables, so claims can cite several file versions.
  - Tables cross the boundary as Arrow.
  - Rows from group and aggregate steps map back to a group key, a member count, and a fingerprint of the member keys.
  - A Parquet parse cache keyed by file fingerprint and parse-spec fingerprint.
  - DuckDB memory and spill settings are explicit.
  - Per-session limits on rows, bytes, and calls.
- **Declarations:**
  - Globs, directories, and list files are expanded and frozen before the run.
  - Output identity is the analysis plus the declared output pattern plus the relative path.
  - Declarations take their shape from DVC's stage format (`cmd`, `deps`, `outs`, `params`).
  - Agent-drafted declarations are refused if any path is outside the project.
- **Spans and drift:**
  - A small raw-text scanner for claim spans.
  - Comparison with normalized whitespace and Unicode NFC.
  - The working tree is the source.
  - Inline code inside a span makes the drift check "not checkable".
  - A lightweight owner action, "update claim text", records the edited sentence and re-runs the checks.
- **Claim flags:**
  - The evidence-moved flags merge into one flag with a reason.
  - Every flag and label has a named clearing action or is marked permanent.
  - Proposal responses return the current flags.
  - A table gives the outcome for each citation type and file situation (missing, removable and deleted, fingerprint only, still hashing).
- **Promotion:** a selection must include everything it depends on. The override is a field on the promote event.
- **agentbox handshake:**
  - The stdio-to-socket bridge is named in the handshake.
  - The socket goes under `/tmp/toulmin-$UID/`, under 107 bytes.
  - Toulmin computes patch-ids itself from the clone (`--stable`, matching commit SHAs first).
  - `tm session close --clone PATH` covers launchers that crashed.
  - An abandoned-session event with a timeout.
  - Agent attribution is computed on demand.
  - "Attribution partial" counts unmatched commits.
  - The must-not-mount list adds the index, the stat cache, query logs, and other sessions' sockets.
- **Agent tools:**
  - Registered file IDs only.
  - Typed refusals naming the rule, the bad field, and the owner command.
  - Draft revision rules.
  - Later sessions see rejections.
- **Runs:**
  - Group array tasks under one run group.
  - A batch-recorded run's `tm run again` prints a resubmit line instead of running on the login node.
  - Record the launch context (inside a batch job, batch script, scheduler).
  - Scheduler variables are captured generically (`SLURM_`, `PBS_`, `SGE_`, `LSB_`).
  - Store the diff of declared scripts, including untracked ones.
  - Git calls use `--no-optional-locks` and only the declared scripts' paths.
- **Staleness sweeps** are cached with their age. `tm status --refresh` re-sweeps.
- **Operations:**
  - `tm doctor` checks filesystem type and mount options, record reachability from compute nodes, clock skew, glibc and kernel, and DuckDB version and extensions.
  - `tm lease status` and `tm lease break`.
  - Status shows pending and quarantined receipts.
- **Formats and policy:**
  - A record format version in the header.
  - Unknown event types are read as opaque, and writing is refused.
  - Receipts and handshake messages carry versions.
  - Reserved slots: parent run, quote and region citations, signatures and collaborators, and a model-check line.
  - An error catalog with stable codes.
  - W3C PROV terms in run events.
- **Explicitly deferred to V1.x:** agents on a separate host (an SSH-forwarded socket plus a token).
- **Dropped:** the query-log retention setting (one deletable file per session) and the I/O-priority promise.

### Drafting work for `/egm-draft` (structure findings, not design choices)

- One decisions register, replacing the four lists in this brief and the hypothesis. Every summary decision is marked kept, changed, or dropped.
- A threat table matched to the stance, plus a table of what each isolation level guarantees.
- Owner-only actions, opt-ins, and labels-and-flags tables.
- A glossary with one meaning per word and the Toulmin term mapping. Renames:
  - "agent-denied files" instead of "files kept away from agents";
  - "segment" instead of "writing session";
  - distinct verbs for accepting a file version and accepting claims;
  - "restore" only for retired files.
- File lifecycle split into identity attributes and version attributes, with allowed combinations. "Cited" is computed.
- One success measure per goal, with build gates drawn from them.
- Assumptions and risks tables.
- A process and deployment section (process table, three setups, the batch-job sequence).
- A command map for a day's work (`tm <object> <verb>`).
- V1 journeys rewritten, including correction, a human writer, and staleness followed by a rerun.
- A Core operations table: actor, permission, library calls in order, events in the batch, and errors.
- A typed read-interface query list, from which the MCP tool schemas are derived.

## Goals vs. Non-Goals

The full goal and non-goal wording is in `toulmin_hypothesis.md` (version 3). In short:

* **In-Scope Goals (V1):**
  - **Linked and checked claims.** Every claim gets an evidence-exists check, and quantitative claims also get an assertion check. Anything uncheckable is shown as "not checkable", never as passed.
  - **Explicit choices.** Hidden choices are required fields in the query step operations.
  - **Evidence and analysis tree at once.** A static view (early) and a live viewer (last) show the evidence and the full analysis tree behind each claim. Approval and review happen there, and owner actions are committed through the CLI or a confirmation step.
  - **Visible methods and agent changes.** Code fingerprints, command, git commit, script diffs, and agent-written commits are recorded, and approval is required where agents changed code.
  - **Safe agent work.** Agents work in journals and draft areas, attributed to their session, with no path to owner actions.
  - **Provenance to the roots,** as far as declarations allow, with honest labels.
  - **Visible upstream changes.** Staleness spreads per analysis with plain reasons, and identical content stops the spread.
  - **Visible new versions.** Re-checks and flags appear on new versions, and the owner accepts claims onto them.
  - **Report text in sync with claims,** through a text-drift check.
  - **No special privileges** on workstations and clusters, including runs inside batch jobs.
  - **Testable reproducibility** with `tm audit`.
  - **Modular and replaceable,** with conformance tests and a Python reference checker.
  - **Imperfect setups still count,** labeled.

* **Out-of-Scope Non-Goals:**
  - Judging scientific validity, detecting cherry-picking, or policing writers.
  - Scheduling jobs, talking to the scheduler, or rerunning chains of analyses. Toulmin wraps the owner's command, and `tm run again` reruns one recorded command.
  - Version control and backup. Git keeps code, large data is fingerprinted, and only files cited by promoted claims are copied.
  - Defending against an owner who gives an agent full rights, or against a determined attacker. The hash chains detect accidental damage, isolation keeps agents out of the record, and unisolated work is labeled.
  - Enforcing that a run reads only its declared files (V1.x). Staleness is only as complete as the declarations.
  - Sharing, signing, collaborators, restriction levels, and export packages (V2).
  - Handling restricted data for the user. Toulmin provides controls and warnings.

## Alternatives Weighed

| Option | Outcome | Reason |
| --- | --- | --- |
| Run every analysis in a container in V1 | Deferred to V1.x | Jobs a workflow tool submits to the scheduler run outside the head process's container; needs a lot of machinery. V1.x options: agentbox presets, Landlock, or an observed mode using `strace` or reprozip |
| Snakemake adapter (read inputs and outputs from Snakemake's records) in V1 | Deferred to V1.x | No required workflow tool in V1; declared runs with snapshot checks cover the need |
| Toulmin owns sandboxing (earlier module M12) | Rejected | agentbox owns launching, mounts, and clones; Toulmin keeps a two-way handshake |
| Fuzzy expected-versus-observed image check | Rejected | A person judges it anyway; an image claim is a qualitative claim with a note on what to expect |
| Record agents' test runs | Dropped | Agents test inside their own sandbox |
| Exploration count | Dropped | It has no reliable meaning across prompts, models, and tasks; the query log stays for diagnostics |
| Signing in V1 | Deferred to V2 | Adds little while an unisolated agent can read the owner's keys |
| An outside anchor for the hash chain | Rejected | The threat stance does not defend against deliberate rewrites; the chain detects accidental damage |
| Export and import in V1 | Deferred to V1.x | `tm audit` plus copying the record covers testing |
| Qualitative claims with no system check | Rejected | An evidence-exists check is reproducible and catches reruns that remove cited rows |
| Detecting unlinked numbers in claim text | Deferred to V1.x | False positives; reviewers send weak claims back |
| Owner-only actions enforced by signatures or an owner socket | Rejected | With no daemon, owner actions are a CLI path plus an owner-only capability type; the live viewer uses a confirmation step |
| Owner actions committed from static pages | Rejected | Static pages cannot write; they produce an action file that `tm apply` confirms |
| A long-lived central service as the only writer | Rejected | Does not fit login-node rules, compute-node runs, or user environments |
| One lease shared by every writer | Rejected | Either blocks the CLI during agent sessions or creates thousands of segments; replaced by per-writer journals plus a CLI-only main chain with epoch fencing |
| PRQL as the step language | Rejected | Pre-1.0, no documented missing-value placement in sort, and a second moving layer; replaced by Toulmin's own JSON operations |
| SQLite as the record | Rejected | Unsafe on network filesystems; SQLite is only a rebuildable index |
| Git-like record internals (refs, merges, one file per record) | Rejected | Inode limits and complexity; append-only segments and journals instead |
| Report text stored as records | Rejected | Markdown in git with Pandoc spans reuses git for drafts and merging |
| Content-hash prefixes or promotion-time aliases as claim refs | Rejected | Session-scoped refs (S4.12, C31) are unique without a central allocator and readable |
| A separate profiles module in V1 | Rejected | Only one profile ships; its rules live in the checker until RNA-seq arrives |
| Automatic conflict detection | Replaced | "Other claims citing these rows" in the evidence view |
| Auto-accept claims on new file versions | Removed from V1 | Owner bulk-accepts; keeps changes visible |
| Approval required for every run | Rejected | Approval fatigue; approval is required only for code with agent-written or unattributed changes and carries over by code version |
| A full hash of every run output on every run | Rejected | Re-hashes unchanged terabytes; only new or changed entries are hashed, with `tm audit` for full validation |
| Separate file-version accept and claim accept | Rejected | Hand-edited inputs are versioned automatically; only claims are accepted |
| Deferring the Python reference checker | Rejected | It is the only independent check on the checker's meaning for inputs nobody anticipated |
| Python, Go, TypeScript, and other languages | Rejected for Rust | Rust's closed enums and exhaustive matching suit Toulmin's many states; libraries (`rmcp`, `duckdb-rs`) are official and current |
| A tilde pin on DuckDB and a glibc-linked build | Revised | An exact pin, extensions locked, and a static musl build so maths results match on every machine |
| Agent-registered inputs as an opt-in in V1 | Deferred to V1.x | No safe upload path across the sandbox yet; agents name files in draft notes and the owner registers them |

## Key Decisions & Staging for Next Steps

### Settled during clarification (history; items 4, 6, and 7 were later revised)

1. V1 includes file fingerprinting and registration with metadata, declared analysis runs, claims with query steps, reports, a GUI for reading and review, and agent sessions.
2. Runs trust declarations, are labeled "declared", and get post-run checks. Container enforcement comes in V1.x.
3. No required workflow tool in V1.
4. Files are expected to stay on disk unless marked removable. Cited files are copied and never removable.
5. agentbox launches and isolates agents. Toulmin records the preset and level, reported by the launcher, never by the agent.
6. Owner-only actions are enforced by connection. Signing moves to V2.
7. Agent-registered inputs are refused by default.
8. Agents query, run query steps, propose claims, draft text, and mark outputs important. All of it goes to a draft area. Agents do not run real analyses.
9. Image claims are qualitative, with a note on what to expect.
10. Two system checks: evidence exists (all claims) and assertion (quantitative claims).
11. Claims cite rows by key. On a new version, easy quantitative claims are re-checked and the rest are flagged.
12. V1.x: PDF and web quote checks, export and import, field profiles beyond the general one, number detection, enforced runs, and workflow adapters.
13. V2: signing, sharing, collaborators, restriction levels, and the packaging wrapper.

### Decided in the blocker walkthrough (Nov 9)

These replace the recommendations for D1 to D7 in the table below.

1. **Threat stance.** Toulmin assumes its users are trying to do good work. It makes the right setup the default and easy, catches accidental changes and changes that were not carried downstream, and makes an agent's shortcuts visible. It does not defend against an owner who gives an agent full rights, or against a determined attacker.
2. **No outside anchor (B2, D3).** The hash chain detects accidental damage, isolation keeps agents out of the record, and sessions without isolation are labeled. Wording per isolation level is honest about this.
3. **Viewer (B1, D2).**
   - The viewer reads and records reviews, protected by a per-launch token, as Jupyter does.
   - Promotion and run approval are CLI only.
   - At setup, Toulmin gives agentbox a must-not-mount list: the record, the owner socket directory, and the token file. agentbox confirms it applied the list. Namespace checks and in-sandbox probes are dropped.
4. **Process model (B3, D1).**
   - No central daemon. Any Toulmin process that writes takes a lease file (host, process ID, heartbeat).
   - The CLI writes directly.
   - Each agent session gets its own small tool server, started by the launcher outside the sandbox.
   - The viewer writes reviews under the same lease.
   - Runs execute in the owner's shell, including inside batch jobs, provided the record directory is on a filesystem both nodes can see.
5. **New goal: upstream changes are visible.**
   - **Triggers.** A run is out of date when its declared script or support scripts, its command line, or any declared input (including config and parameter files) differs from what it used. The software environment is recorded but is not a trigger by default.
   - **Code fingerprints.** Declared support scripts stay, so that unrelated commits do not mark every run out of date. The git commit is recorded for provenance only.
   - **Spread.**
     - A run whose own inputs or code changed is "changed".
     - Runs that read outputs of changed or out-of-date runs are "upstream out of date", and claims citing affected files inherit the label.
     - A rerun with byte-identical outputs stops the spread.
   - **Reporting.**
     - Toulmin lists out-of-date runs in dependency order, with their recorded commands. It never reruns a chain itself.
     - `tm run again R12` reruns one recorded command with today's scripts and inputs, as a new run linked to the old one. It warns if the current environment differs from the recorded one.
   - **When checks happen.** Checks run on demand (`tm status`, session start, the viewer), using a stat cache. A deleted removable file counts as expected-missing.
   - **Agents and approvals.** Agents see staleness through the same queries, and their claims on stale files are labeled. Approving a run with out-of-date inputs warns.
6. **Run check (B4, D6).**
   - Before the run: fingerprint the scripts, record the command and git commit, and snapshot declared inputs and outputs.
   - After the run: snapshot again and compare snapshot to snapshot (inode and change time), never to a clock.
   - Outputs not rewritten that match an earlier version are labeled "carried forward". Inputs changed during the run are labeled.
   - The head process must wait for its jobs (`tm run` inside the batch script).
7. **File states (B5, D4).**

   | State | How a file gets there |
   | --- | --- |
   | Tracked | Automatically, as a declared input or output |
   | Registered | Given a title and description; citable |
   | Cited | A claim, draft or promoted, points at a version; its fingerprint is pinned |
   | Kept | The first promoted claim cites it, or the owner keeps it on purpose; its bytes are copied into the record |
   | Removable | The owner marks it; the on-disk file may be deleted |
   | Retired | Its analysis no longer declares that output; the owner can restore it |

   - An agent's "mark important" is a proposed title and description in its draft.
   - A file changed between an agent's draft and promotion flags the claim "evidence changed since drafting".
   - Keeping a file above a size threshold asks for confirmation. The owner may keep only the fingerprint, and the claim is labeled "evidence not kept".
8. **New versions (D5).**
   - **Identity:** a run output is its analysis plus its declared output name; an input is its project path, and renaming it needs an explicit command.
   - **What counts as new:** different bytes make a new version. Identical bytes, or a table identical after sorting by key, are carried forward with no flags.
   - **Inputs changed outside a run** are flagged; the owner accepts them as a new version or restores the file.
   - **Claims on a new version:**
     - Quantitative claims whose steps still run and whose keys are still found are re-checked against the new version ("holds" or "fails"). Other claims are flagged "cites an older version".
     - Claims move to a new version only when the owner accepts, in bulk; agents may propose this in a draft.
     - There is no auto-accept option in V1.
9. **Fingerprints.**
   - **The fingerprint** is a full SHA-256 of the contents, with an algorithm prefix on every digest. A stat cache (device, inode, size, modified time, change time) decides when to re-hash.
   - **A full hash always runs** on first tracking, on every run output, on every copy into the record, and on `tm scan --full`. Directories get a hash over their sorted entries.
   - **Hashing runs** in parallel at low I/O priority, alongside the analysis. Run receipts store stat snapshots first, so `tm run finish R12` can complete hashing after a job hits its time limit. New raw data hashes in the background and is not citable until done.
   - **No partial hashing in V1.** A provider's checksum (for example, an MD5 from ENA or SRA) can be stored and checked once at registration.
10. **Agent code attribution (D7).**
    - At session end, agentbox reports the base commit, final commit, and each commit's `git patch-id`. Runs mark matching commits as agent-written.
    - The `agent:<session>` git author is shown but not trusted.
    - Agents run outside agentbox are not detected. Setup recommends agentbox.
    - Squashing agent commits together with owner edits loses attribution for that change.
11. **Step language (D8).**
    - **Operations.** Toulmin owns a fixed set of about eight JSON operations: filter, select, sort, take, rank, derive, group and aggregate, and join by key. Each operation is versioned separately, and its meaning never changes.
    - **Required choices are fields:**
      - rank: tie rule and missing-value placement;
      - sort: missing-value placement, with the key always appended as the last tie-breaker;
      - filters treat missing values as not matching.
    - **Translation.** Toulmin translates the steps into fully explicit DuckDB SQL (null placement, tie-breakers, one thread) and stores the SQL with each verdict for debugging. Every verdict records the DuckDB version, and a DuckDB upgrade is a deliberate step followed by a report of changed verdicts.
    - **PRQL was rejected:** it is before version 1.0, its sort reference documents no missing-value placement, and it would add a second moving layer. A text front end that writes the same JSON may come later.
12. **Record storage (D9).**
    - **Source of truth:** append-only segment files of JSON events, each carrying the previous event's hash, with a new segment per writing session.
    - **Kept files:** stored compressed in a content-addressed folder (`objects/sha256/...`).
    - **SQLite** is only a rebuildable query index and may live on local disk. It is not the record, because SQLite is unsafe on network filesystems.
    - **Drafts and promotion.** Drafts are a session field on events. Promotion is an owner event pointing at the draft events it accepts, so promotion can be partial.
    - **Record IDs** are SHA-256 of canonical JSON (RFC 8785).
13. **Clusters and schedulers.**
    - **Toulmin never talks to the scheduler.**
    - **Single scripts:** `tm run` goes inside the batch script, recording the job ID, node, partition, and requested resources.
    - **Workflow tools:** wrap the head process of Snakemake or Nextflow in `tm run` and submit that as a batch job. It counts as one run until the V1.x adapters add per-rule lineage.
    - **Job arrays:** use a workflow tool, or `tm run --part $SLURM_ARRAY_TASK_ID` for one run per task.
    - **Inbox.** Compute nodes drop receipts into an inbox folder in the record directory, so they never take the write lock. The next `tm` command on the login node reads them in.
    - **Submission commands.** Bare `sbatch` warns; `sbatch --wait` is accepted.
    - **Late outputs.** A configurable wait handles outputs that appear late on shared storage.
    - **Other schedulers.** PBS, SGE, and LSF work the same way, with their own environment variables recorded.
14. **Report text (D10).**
    - **Reports are Markdown files in git,** registered with `tm report add`.
      - Section kinds are heading classes (`## Results {.results}`).
      - Claims are Pandoc spans (`[sentence]{.claim #C12}`) pointing to claim records.
    - **Text drift.** Toulmin stores each claim's sentence. If the span text in the file differs from it, the claim is flagged "text changed since check".
    - **Agents** write text in their clone and commit it. Their claims go to the draft area under the same IDs, and the owner merges the text and promotes the claims. A span pointing to an unpromoted claim shows as unpromoted.
    - **Human writers** use the same span plus `tm claim add` with a JSON steps file.
    - **Versions and reviews.** Reviews record the report file's hash. Files stay plain Pandoc and Quarto Markdown.
15. **Owner actions (D13).**
    - **Review:** confirmed, disputed, or question, with a note, on a claim or run. It is a separate line from system checks. The owner reviews from the CLI or viewer; collaborators come in V2.
    - **Promote:** CLI only, all or selected draft items. Promoted items show "promoted, not reviewed" until reviewed. `--confirm` also records a confirmed review.
    - **Send back:** marks draft items rejected, with a note. Nothing is deleted, and later agent sessions on the project see open rejections through their queries.
    - **Approve run:** CLI only. It records that the owner reviewed the run's code changes (including agent-written ones) and its checks; a failed check needs a note. Claims citing outputs of unapproved runs cannot be promoted without a recorded override.
    - **Retract and supersede:** the owner withdraws a claim with a reason, or replaces it with a new claim that points back. Agents may propose a supersede in a draft.
    - **The owner's own claims** go straight into the record.
16. **Modules (D11).** Five synchronous libraries (Record, Files, Runs, Checker, and Claims), plus a Core of user-level operations, permissions, batch writes, and one read interface, plus three thin front ends: the CLI, the agent tool server, and the viewer.
    - Libraries never call each other; the Core passes data between them.
    - Formal conformance tests cover the shared formats, the Record, and the Checker.
17. **Scope cuts (D12).**
    - **Folded or replaced:**
      - Profiles fold into the Checker as its general rules.
      - Conflict detection becomes "other claims citing these rows".
    - **Deferred:** agent-registered inputs move to V1.x, since agents name files in draft notes and the owner registers them.
    - **Moved out of the permanent record:** the agent query log is a per-session diagnostic file outside the permanent record, storing fingerprints and row keys, with a retention setting.
    - **Added:** agents can draft an analysis declaration, run by the owner with `tm run --from-draft`.
18. **Shared-format defaults (B6).**
    - **Record scope and location:** one record per project. The location is set at `tm init`, outside the repository by default, and is on the must-not-mount list.
    - **Owner identity:** a random owner ID.
    - **Citations:** cells (file version, key columns and values, columns), whole file, page (checked against the page count), and panel (a label registered on that image version). Text quote, page region, and image region are reserved for V1.x.
    - **Check and review values:**
      - Each system check is holds, fails, not checkable, or error.
      - Reviews are confirmed, disputed, or question.
      - A mixed claim gets both checks plus review.
    - **Roles:** one agent role.
19. **Language (D14): Rust.**
    - **Why:** closed enums with exhaustive matching suit Toulmin's many states. There are no null values, errors must be handled, and the libraries needed are official and current: `rmcp` (Tier 1 MCP SDK) and `duckdb-rs` (DuckDB organization).
    - **Conditions:**
      - Async confined to the agent tool server and viewer.
      - Canonical JSON written in-house and tested against the RFC 8785 examples.
      - `cargo-deny` and `cargo-audit` in CI, with an allowed-crate list.
      - DuckDB pinned with a tilde requirement.
      - Release builds against an old glibc.
      - Clippy at pedantic level; no `unwrap` in libraries; `unsafe` only in the DuckDB binding.
      - Distinct ID types, state enums, and an owner-only capability type the agent server never holds.
    - **Testing:** a deliberately simple Python reference checker for differential testing, plus conformance and scenario tests written so the owner can read them.

### Decided in the second-review walkthrough (Nov 9)

20. **Writer model (D15).**
    - **Journals.** Each agent tool server, the viewer, and the run wrapper append to their own journal or receipt. Each has one writer and its own hash chain, and none needs a lock.
    - **The main chain.** Only the CLI writes it, under a short `O_EXCL` lock with an epoch number. Before each write, the CLI checks that no higher epoch exists, which stops a paused writer that woke up after a takeover. Takeover requires an unchanged heartbeat for several minutes, judged by the local monotonic clock.
    - **Reading journals in.** Every `tm` command reads new journal lines and records each journal's head hash in the main chain. The index reads the main chain plus open journals.
    - **Promotion and drafts.** Promotion is a main-chain event pointing at journal events. Unpromoted and sent-back drafts stay in their journals permanently.
    - **Crashes.** A torn last line is skipped by readers and reported by `verify` as a torn tail.
21. **Claim refs (D16).**
    - **Format:**
      - Sessions get a short number at `tm session start`, taken under the lock (S4).
      - Each tool server numbers its own claims (S4.1, S4.2), and the owner's CLI numbers the owner's claims (C31).
    - **Spans and internal IDs.** Spans use an attribute, `[sentence]{.claim ref=S4.12}`, so one claim can appear more than once. The internal ID is the hash of the creation event and never changes.
    - **Revisions** keep the ref, with each revision a new event. The history stays visible, including assertion changes.
    - **Supersede** gives a new ref. Old spans show "points at a superseded claim; replacement ..." with the drift flag.
    - **No separate display aliases.**
22. **Viewer (D18).**
    - **Static first.** `tm view --out DIR` is built early (gate A). It writes static pages for reports, claims, files, and runs, with copyable review commands. The export lists which files its rows came from and honours the agent-denied list.
    - **Live viewer last.** The live viewer is built at gate D and stays in V1, with a per-launch token, Host and Origin checks, sanitized Markdown, and a strict content security policy.
    - **Static stays supported** as a permanent option, for users with unreliable networks or who prefer it.
    - **Owner actions (amended by D17):** committed from the CLI, or through the live viewer's confirmation step. Never from agent tools or static pages.
23. **Run approval (D17).**
    - **Meaning.** Approval means "I reviewed the code that produced these outputs, including agent changes, and it is the analysis I intended". It does not certify that outputs are correct; claim checks cover that.
    - **When it is required:**
      - for code versions with agent-written changes, or with unattributed changes made while a session was open;
      - code versions with only owner changes are approved automatically and labeled "owner code, not separately reviewed";
      - a setting requires approval for everything.
    - **Scope and carry-over.** Approval covers the declared script and support-script fingerprints plus the command template. Reruns with the same code inherit it; a failed run check on a rerun cancels the carry-over.
    - **Promotion gate.** Claims citing outputs of unapproved code cannot be promoted without `--allow-unapproved`, which is recorded on the promote event.
    - **Where you review.** Every claim page in both views shows the analysis tree from the cited file version back to registered inputs. Each run shows its code version, approval state, diff since the last approved version (agent lines marked), run checks, and staleness. Scientific review of the claim sits on the same page.
    - **How you commit:**
      - **Static view:** checkboxes collect approvals, promotions, and reviews into an action file. `tm apply FILE` shows a summary and asks for confirmation.
      - **Live viewer:** actions are committed directly through a one-time server-issued confirmation code plus the per-launch token. The event records that the viewer was the source.
24. **Hashing tiers (D19).**
    - **Routine checks** (`tm status`, session start, the viewer) use stat only: size, modified time, change time, and inode. Files whose stats changed are hashed on the spot if under a size threshold; otherwise they are listed as "possibly changed (unconfirmed)".
    - **`tm run`** fully hashes every input and output that is new or whose stats changed, and reuses recorded hashes for unchanged files.
      - Forced fresh attributes after a run, and git's racy rule, cover the known stat gaps.
      - Directory manifests are stored once and reused when unchanged.
      - One hashing thread by default inside batch jobs, with a setting for more threads and a bandwidth cap. Hashing time is recorded.
      - SHA-256 only in V1.
    - **`tm audit`** (on request, typically before review) is a full validation. It replaces `scan --full` and `verify --recheck`.
      - It re-hashes all tracked files and kept copies, checks the chain and journals, and re-runs every claim check against stored verdicts.
      - It lists out-of-date runs, unapproved code, open flags, and unpromoted drafts.
      - It writes a JSON and HTML report explaining each hash mismatch (last confirmed time, dependents) and each verdict change (data, DuckDB, or checker version).
      - `--since LAST` checks only what changed since the previous audit.
    - **Directory noise:**
      - Claims normally cite files inside a directory; identity is the analysis plus the declared pattern plus the relative path.
      - Declarations can exclude files, and the exclude list counts as part of the declaration.
      - A configurable, recorded default ignore list covers `.snakemake/`, `.nextflow/`, `__pycache__/`, `*.swp`, and `.DS_Store`.
      - Directory changes report exactly which entries were added, removed, or modified.
      - Downstream staleness follows each run's declared inputs. `tm status` suggests narrowing a directory input that keeps changing in files the run never reads.
25. **Build gates (D20):**
    - **Gate A:** shared formats, Record, Core and CLI, Files (inputs), Checker with conformance cases, Claims and report spans, static view, and `tm audit`.
    - **Gate B:** agent tool server, stdio bridge, handshake and unisolated sessions, journals, refs, promote, send back, and `tm apply`.
    - **Gate C:** Runs, staleness, new versions, approval, attribution, run groups, and drafted declarations.
    - **Gate D:** the live viewer.

    The first real test is re-deriving the claims of one of the owner's published papers from its registered tables.
26. **Staleness (D22).**
    - **What goes out of date.** Staleness belongs to an analysis (its latest successful run). Each flag gives one plain reason: own code changed, own inputs changed, command changed, or upstream changed.
    - **Spread cutoff.** The spread stops at identical content: the same bytes, the same table sorted by key, or the same decompressed content for `.gz` and `.bgz`.
    - **Commands.** Command templates mark which parts may vary without counting as a change, and the expanded command is still recorded.
    - **Hand-edited inputs** become a new version automatically, labeled "changed outside a run". There is no separate file accept, only the claim accept. Restoring means the owner restores the bytes, and Toulmin shows the expected fingerprint.
    - **Failed or interrupted runs** do not make their outputs current. A declared output that was not produced fails the run check, and removing it from the declaration retires it.
    - **Lineage** is tracked per version, so a run that updates a file in place never creates a loop.
    - **Noisy formats.** The docs recommend `gzip -n` and `SOURCE_DATE_EPOCH`. Format-aware comparison (BAM `@PG` lines, PDF dates) is V1.x.
27. **Checker test suite (D21).** The suite has five parts:
    - hand-checked conformance cases;
    - property and metamorphic tests (`proptest`);
    - differential tests against a deliberately simple Python reference checker (plain Python, no pandas), written from the spec alone by a separate agent session at gate A, with last-digit differences for log and exp reported rather than failed;
    - snapshot tests of the generated SQL;
    - fuzzing of the parsers.

    A second Python implementation of canonical JSON and IDs cross-checks the record format.
28. **Agents outside agentbox (D23).**
    - **Unisolated sessions.** `tm session start --unisolated --harness NAME` gives a labeled session and draft area, and prints the MCP setup using the same stdio bridge.
    - **Labels on owner actions.** Owner actions record whether they came from an interactive terminal. A known agent-harness environment variable (for example, `CLAUDECODE=1`) triggers a confirmation and a "possibly agent-issued" label.
    - **Warnings.** `tm status` warns when owner actions happen while an unisolated session is open.
29. **DuckDB and the build (D24).**
    - **DuckDB:** pinned exactly, with a committed lock file. Upgrades are deliberate and followed by `tm audit`. Extension auto-install and auto-load are off, Parquet is compiled in, and each verdict records the runtime version and loaded extensions.
    - **Build:** a fully static musl build with a faster allocator. The fallback is glibc 2.17 with static `libstdc++`.
    - **CI:** checks symbol versions and smoke-tests on Rocky 8 and 9 and CentOS 7.
    - **First task:** a spike proving DuckDB builds and runs under musl.
30. **Files and Runs boundary (D25).**
    - **A shared filesystem-facts component with no state.** It covers stat snapshots, hashing, directory manifests, exclusions, and the racy rule, and is used by Files and by the run wrapper.
    - **Run wrapper.** It is a Core operation that never takes the lock.
    - **Files** receives facts and keeps identity, versions, states, and parse specs.
    - **The Core** owns staleness and all permission checks. Libraries receive plain, already-authorized decisions.

### Original open decisions (all decided above; kept for history)

| # | Decision | Recommendation | Source |
| --- | --- | --- | --- |
| D1 | Process model | One writer per record, held by a lease. The CLI hosts the writer when no service runs; the service starts on demand for agents and the viewer. Runs execute in the owner's shell and hand in a receipt | B3 |
| D2 | Viewer write powers in V1 | Read and record reviews only; promotion and approval in the CLI | B1 |
| D3 | Tamper detection | Outside anchor at each promotion and approval, plus honest wording per isolation level | B2 |
| D4 | What makes a file citable, and when bytes are copied | Registration makes it citable; bytes are copied on first citation by a promoted claim; agent drafts pin fingerprints only | B5 |
| D5 | New version semantics | Re-check writes a verdict against the new version; the claim stays on the old version until the owner bulk-accepts; unchanged content is carried forward; no auto-accept opt-in in V1 | M2 |
| D6 | Run check design | Before-and-after stat snapshots, git commit recorded, "carried forward" label, blocking head process required | B4 |
| D7 | Agent code attribution | Launcher reports commits and patch IDs at session end; each run records its git commit | M1 |
| D8 | Step language | PRQL, or a small JSON operation list if PRQL is rejected | M4 |
| D9 | Record storage | SQLite event chain plus a content-addressed folder; drafts as a column | M9 |
| D10 | Where report text lives | Markdown in git with Pandoc spans; claims keep the draft area | M6 |
| D11 | Module count | About five parts, with separate interfaces inside each so later splits are mechanical | M9 |
| D12 | Smaller scope cuts | Fold profiles into the checker; replace conflict detection with "other claims citing these rows"; drop the agent-input opt-in | M9 |
| D13 | Owner correction and approval semantics | Owner retract and supersede in V1; define what promote, review, approve, and send back each change | M6 |
| D14 | Implementation language | Not yet stated; the earlier spec used Python | B6 |

### Staging

Done:
- The blockers and all decisions (D1 to D25) are resolved.
- The review inputs were refreshed twice, and the review was re-run once.
- Version 3 of the hypothesis and module list consolidates everything.

Next:
1. **Run `/egm-draft`** with this brief, `toulmin_hypothesis.md` (version 3), and `toulmin_modules.md` (version 3) as inputs. The draft must include the drafting work listed under the second review:
   - one decisions register replacing the lists in this brief;
   - a threat table and isolation guarantees;
   - owner-only actions, opt-ins, and labels-and-flags tables;
   - a glossary with the Toulmin term mapping;
   - the file lifecycle split into identity and version attributes;
   - success measures per goal, with build gates drawn from them;
   - assumptions and risks;
   - process and deployment;
   - a command map;
   - rewritten journeys;
   - a Core operations table;
   - a read-interface query list.
2. **Write the shared-formats decision sheet**, the first section of the draft. It is needed before any code: envelope fields, IDs and the chain link, batch framing, journal and receipt formats, typed values, the assertion schema, text sort order, the version policy, the error catalog, reserved fields, and the handshake schema.
3. **First build task:** the musl spike for DuckDB, then gate A.
