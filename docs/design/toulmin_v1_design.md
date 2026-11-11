# Toulmin V1: architecture design

*   **Author:** Tim (Timothy Stephens)
*   **Status:** Proposed
*   **Date:** 2026-11-09
*   **Inputs:** `concepts_brief.md` (final, Nov 9), `toulmin_hypothesis.md` (version 3), `toulmin_modules.md` (version 3). `Summary.md` (Nov 9) is background for the problem statement only; where its decisions differ from version 3, the Decisions Register says what happened to each.
*   **Scope of this document:** the whole V1 architecture, with a Detailed Implementation Plan for build gate A only. Gates B, C, and D are listed under Future Improvements as follow-on design documents, with every gate's exit criteria.

--------------------------------------------------------------------------------

## Overview

Toulmin is a single-user, command-line tool, written in Rust, for scientists on Linux workstations and shared clusters. It links every claim in a scientific report to the exact evidence behind it: a registered file version, the declared query steps applied to it, the rows or region those steps produce, and the history of the analysis runs that made the file. The system checks what it can check (the cited evidence still exists, and a quantitative assertion still holds), reports what it cannot check as "not checkable", and leaves scientific judgment to the owner.

It is built first for reports drafted by sandboxed AI agents and reviewed by the scientist who owns the project. Agents work in their own journals and draft areas. They can query registered files, run query steps, draft claims and report text, propose titles and descriptions, and draft analysis declarations. They cannot approve, promote, register inputs, run analyses, or touch the record. The owner reviews each claim with the full analysis tree behind it, in a static view or a live viewer, and commits every owner action through the CLI or a confirmation step.

Toulmin runs alongside git and does not replace it. Git keeps code and report text. Toulmin fingerprints data files, records declared analysis runs, tells the owner which analyses are out of date after an upstream change, and copies into its record directory only the files that promoted claims cite.

There is no daemon. Every writer owns its own file: the owner's CLI writes the main event chain under a short lock, each agent tool server and each viewer launch writes its own journal, and the run wrapper writes receipts. The owner's CLI reads journals and receipts into the main chain whenever it takes the lock.

Sandboxing is not Toulmin's job. agentbox, a separate project, launches and isolates agents; Toulmin exchanges a versioned handshake with it and records the isolation level of every session. Agents run without agentbox get a labeled, unisolated session.

### Threat stance

Toulmin assumes its users are trying to do good work. It:

- makes the right setup the default and easy;
- catches accidental changes, and changes that were not carried downstream;
- makes an agent's shortcuts visible: editing a script until a claim passes, hand-editing a table, or loosening an assertion.

It does not defend against an owner who gives an agent full rights, or against a determined attacker. Hash chains detect accidental damage and partial edits. Isolation keeps agents out of the record directory. Work done without isolation is labeled, and during an unisolated session "who did what" is not reliable. There is no outside anchor and no signing in V1, so a full rewrite of the record by an agent running as the owner is not detected.

## The Problem

Checking a claim in a report today means rebuilding the writer's work by hand. A report says "Gene X is upregulated under condition Y versus Z, and has the fifth highest fold change" and cites "Dataset 1". The reviewer opens the table, guesses the filter and the sort, finds the gene, and compares numbers. That is slow for the reviewer and error-prone for the writer. Typical slips are copying the p-value instead of the adjusted p-value, reading a neighboring column, or dropping "likely" from an annotation.

AI agents sharpen the problem in two ways:

- **Speed.** Agents produce analyses and drafts faster than a person can retrace them by hand.
- **Hidden choices.** An agent's filters, thresholds, ranking scope, and handling of ties and missing values are rarely stated, so the same sentence can rest on very different analyses.

Existing executable-article tools link code to a paper as a whole. They do not tie each sentence to the exact cells that support it at the moment it is written, and they do not track the full history of an analysis. Workflow tools (Snakemake, Nextflow) run analyses and record their own dependency graphs, but they stop at the file: nothing connects a file to the sentences that depend on it, and nothing tells the writer that a sentence written last month now rests on a file that was regenerated yesterday.

The setting adds constraints that rule out the obvious server-based design. Research runs on personal workstations and shared clusters without administrator rights. Login nodes kill long-running processes, users land on different login nodes from one session to the next, compute nodes cannot reach a Unix socket on a login node, and the record often lives on a network filesystem where SQLite and `flock` are unsafe. Anything that needs a daemon, a database server, or root is out.

The proposed solution is a set of synchronous Rust libraries behind a thin Core, with three front ends (CLI, agent tool server, live viewer) and one static-view output. The record is an append-only chain of JSON events in segment files, plus per-writer journals, a receipt inbox, and a content-addressed folder for kept files. A rebuildable SQLite index on local disk serves queries. The Checker is a pure function from Arrow tables and JSON query steps to a verdict, translated to explicit DuckDB SQL under an exact DuckDB pin and a static musl build, so a verdict does not depend on the host's maths library or DuckDB build (the glibc fallback in Risks and Mitigations is the stated exception).

## Goals

Each goal carries one testable success measure. The gate exit criteria under Future Improvements are drawn from these measures.

| ID | Goal | Success measure |
| --- | --- | --- |
| G1 | **Every claim is linked.** Every claim gets an evidence-exists check, defined per citation type, and quantitative claims also get an assertion check. Anything the system cannot check is shown as "not checkable", never as passed. | In the reference project (one of the owner's published papers re-derived from its registered tables), every claim has a recorded verdict, and a scripted scan finds no claim shown as "holds" whose check result is "not checkable" or "error". |
| G2 | **Hidden choices become explicit.** Tie rules, missing-value handling, and rank scope are required fields in the query step operations. | The Checker's schema validation rejects every conformance case that omits a required choice, and the Python reference checker rejects the same cases. |
| G3 | **Reviewers see the evidence and the analysis tree at once.** The static view is built early and stays a permanent option; the live viewer is built last. Both show each claim's evidence and the full tree from the cited file version back to registered inputs. | For every claim page in the static view of the reference project, the page contains the cited rows in their full row context, each step's output, and a tree whose leaves are all registered inputs or files labeled `root not declared`. |
| G4 | **Methods can be reviewed.** Each run records its declared script and support-script fingerprints, the command template and expanded command, the git commit, the diff of declared scripts (including untracked ones), and which commits an agent wrote. Approval is required where agents changed the code. | A scenario test in which an agent session commits a one-line script change produces a run whose code version carries `approval required`, whose diff marks the agent line, and whose outputs' claims cannot be promoted without `--allow-unapproved`. |
| G5 | **Agents do the tedious work safely.** Agents query, run query steps, draft claims and report text, propose titles and descriptions, and draft analysis declarations. Everything lands in the session's journal, attributed to the session. Agents have no path to owner actions. Isolation guarantees are stated per level, and unisolated sessions are labeled. | CI fails if the owner-capability constructor feature is enabled in the agent tool server's or bridge's dependency tree; the read-in allowlist test shows that a forbidden event type in a journal is quarantined, not applied; a scenario test calls every MCP tool with every owner-action payload shape and gets a typed refusal for each; no event in any session journal is of an owner-action type. |
| G6 | **Provenance goes back to the roots,** as far as declarations allow, with labels for what was declared, carried forward, or changed outside a run. | For every kept file version in the reference project, `tm file lineage` ends at registered inputs or at a `root not declared` label, never at an unlabeled unknown. |
| G7 | **Upstream changes are visible.** An analysis goes out of date when its code, command, or inputs change, or when something upstream does. Each flag gives a plain reason. Identical content stops the spread. | A scenario test edits one script in a three-stage chain; `tm status` lists the edited analysis as `own code changed` and both downstream analyses as `upstream changed` in dependency order. A rerun that reproduces identical output clears the downstream flags. |
| G8 | **Files describe themselves** with a stable title, description, and source. | `tm file register` refuses a file without a title and description; every citable file in the reference project returns all three fields from the read interface. |
| G9 | **Everything is queryable before writing,** through one read interface shared by the CLI, agents, and both views. | Every query in the read-interface list has a CLI subcommand, an MCP tool (where agents are allowed it), and a static-view page that return the same data from the same Core function; each static page embeds its source rows as a JSON block, and a test compares all three for the test fixture. |
| G10 | **New versions of cited files are visible.** Quantitative claims are re-checked against the new version, other claims are flagged, and the owner accepts claims onto the new version. | A scenario test produces a new version of a cited table; every quantitative claim citing it gets a new verdict against the new version, every other claim gets `evidence moved` (newer version exists), and no claim changes its cited version until `tm claim accept`. |
| G11 | **Report text and claims stay in sync.** A claim whose sentence in the report differs from its recorded sentence is flagged. | A scenario test edits one span's text in the report; `tm status` flags exactly that claim with `text drifted`, and `tm claim update-text` clears it. |
| G12 | **Works without special privileges** on a workstation or a shared cluster, including runs inside batch jobs. | The release binary runs with no installation step beyond copying it, on CentOS 7, Rocky 8, and Rocky 9 in CI, and a `tm run` inside an `sbatch` script on a compute node that can see the record directory produces a complete two-phase receipt (checked on the owner's cluster and recorded as a manual gate C check, since CI has no scheduler). |
| G13 | **Validation on request.** `tm audit` re-hashes everything, checks every chain, re-runs every claim check, and reports what changed and why. | On the reference project, `tm audit` run twice with no changes in between reports zero differences; after each listed tampering case (a byte changed in a kept file, a line removed from a segment, a journal truncated, a verdict's data changed), it reports that case by name. |
| G14 | **Modular and replaceable,** with conformance tests for the shared formats, the Record library, and the Checker, and a Python reference checker for differential testing. | The conformance suites run against the Rust implementation and the Python cross-check with identical results; differential testing over at least 10,000 generated cases shows no difference in any field, the result hashes included. |
| G15 | **Imperfect setups still count,** labeled as such. | An unisolated session, a run with an undeclared input, and a claim on unapproved code each complete and each carry the matching label in the static view. |

## Non-Goals

- **Judgment.** Judging scientific validity, detecting cherry-picking, or policing writers.
- **Running workflows.** Scheduling jobs, talking to the scheduler, or rerunning chains of analyses. Toulmin wraps the owner's command; `tm run again` reruns one recorded command.
- **Version control and backup.** Git keeps code. Large data is fingerprinted, and only files cited by promoted claims are copied into the record.
- **Deliberate tampering.** Defending against an owner who gives an agent full rights, or against a determined attacker (see the threat stance in the Overview).
- **Enforced reads.** Enforcing that a run reads only its declared files (V1.x). Staleness is only as complete as the declarations.
- **Sharing.** Signing, collaborators, restriction levels, and export packages (V2).
- **Restricted data.** Handling restricted data for the user. Toulmin provides controls and warnings.

## Open Questions

Every design decision in the three input documents is settled and is not reopened here. The four D15 and D16 gaps found at drafting time were decided with the owner at drafting time and are recorded in the Decisions Register (D26 to D29). The items below are the ones none of the inputs answer. Each has a proposed default that the draft uses until the owner says otherwise.

| # | Question | Proposed default in this draft |
| --- | --- | --- |
| OQ1 | What is the size threshold below which a stat-changed file is hashed on the spot during routine checks (`tm status`, session start, the viewer)? | 64 MiB, a setting. |
| OQ2 | How long must the main-chain heartbeat be unchanged before `tm lock break` or an automatic takeover is allowed? | 5 minutes on the local monotonic clock (setting `lock.takeover_seconds`); `tm lock break` always asks for confirmation. |
| OQ3 | After how long without an end report is a session marked abandoned? | 72 hours, a setting; `tm status` shows open sessions with their age before that. |
| OQ4 | What are the default per-session limits on rows, bytes, calls, and time for the agent tool server? | 10,000 rows per query result, 64 MiB returned per call, 5,000 calls per session, 30 seconds of DuckDB time per call (`interrupt` on expiry), all settings. |
| OQ5 | What is the size warning threshold before a file is kept, and is there a per-project byte budget for kept files? | Warn above 1 GiB per file; a project budget setting, unset by default, that refuses `keep` above the budget. |
| OQ6 | Where does the project-side pointer to the record directory live, and is it committed to git? | `.toulmin/project.json` in the repository root, committed, holding the project ID only. The record path is bound outside the repository, in `~/.config/toulmin/projects/<project-id>.json`, written by `tm init` (or `tm init --attach RECORD_PATH` for a second checkout or another machine, which writes only the binding after checking `record.json`) and on the must-not-mount list. The binding lists the repository roots it was created for. `tm` refuses to run when the project IDs in `project.json`, the binding, and the record's `record.json` disagree (`E2004` binding missing), or when the current repository root is not in the binding (`E2005`). |
| OQ7 | Which agent-harness environment variables trigger the `possibly agent-issued` confirmation and label? | `AGENTBOX_SESSION` (set in every agentbox session) and `CLAUDECODE` (set by Claude Code itself); `AGENTBOX_AGENT` names the harness when present; extendable by the `harness.env_vars` setting. |

## Critical User Journeys (CUJs)

Commands follow the `tm <object> <verb>` map given in the Technical Plan. Handles used in the journeys: `S4` is agent session 4, `S4.12` is the twelfth claim proposed in that session, `C31` is the owner's thirty-first claim, `R12` is run 12, `A3` is analysis 3, and `F7@v2` is version 2 of registered file 7.

### CUJ 1: Register raw data

*   **Given**: A git repository with `.toulmin/project.json` holding the project ID, and a binding in `~/.config/toulmin/projects/` pointing at a record directory on a filesystem that compute nodes can also see (`tm init --record PATH` created all three). A directory `data/raw/` holds FASTQ files and a sample sheet `data/samples.tsv` downloaded from ENA, with the provider's MD5 list.
*   **When**: The owner runs `tm file register data/samples.tsv --title "Sample sheet, cold stress experiment" --description "..." --source "ENA PRJEB12345"`, then `tm table spec data/samples.tsv --key sample_id --types ... --missing "NA,"` and `tm file register data/raw/ --title "Raw reads" --description "..." --source "ENA PRJEB12345" --checksums data/raw/md5.txt`.
*   **Then**: Each file gets an identity (its project path), a title, a description, and a source. `samples.tsv` is hashed on the spot, its key is checked for uniqueness, and its frozen parse spec is recorded; a duplicate key refuses the spec with the offending values. `data/raw/` was registered with `--defer-hash`, so it gets an identity, title, and description but no version yet and is not citable; the owner runs `tm file hash F9` inside a batch job, which writes the directory manifest with one hashing thread, compares the provider checksums once, and adds the first version. Without `--defer-hash`, `tm file register` hashes in the foreground. `tm file show F7` returns title, description, source, versions, and attributes.

### CUJ 2: A cluster run in a batch job

*   **Given**: Analysis `A3` is declared (`tm analysis declare de --cmd "Rscript scripts/de.R {threads}" --deps scripts/de.R data/counts.tsv config/de.yaml --outs results/de/ --params threads`), its support script `scripts/lib.R` is declared, and `results/de/` excludes `*.log`. The record directory is reachable from compute nodes.
*   **When**: The owner submits `sbatch run_de.sh`, where the script contains `#SBATCH --signal=B:TERM@120` and ends with `exec tm run A3 --param threads=$SLURM_CPUS_PER_TASK`, so the signal reaches `tm run` directly (the user guide also shows the trap-and-wait form for scripts that cannot `exec`). The job runs on a compute node.
*   **Then**: Before the command starts, the wrapper fingerprints `scripts/de.R` and `scripts/lib.R`, records the command template and the expanded command, the git commit, the diff of declared scripts, the launch context (batch job, script path, scheduler), and all `SLURM_` variables, snapshots the declared inputs and outputs, and writes a "started" receipt into the record's inbox by write-then-rename. After the command exits, it snapshots again, hashes every output that is new or whose snapshot changed, reuses recorded hashes for unchanged ones, and writes a "finished" receipt with exit status, hashes, and hashing time. If the job is killed, the `SIGTERM` handler forwards the signal to the child, waits up to 30 seconds for it to exit, snapshots the outputs, writes a partial receipt, and exits; `tm run finish R12` completes the hashing later, printing a resubmit line instead when the recorded hashing estimate exceeds the login-node limit. The next `tm` command the owner runs on the login node (for example `tm status`) reads the receipts in, creates the run, applies labels (`content changed`, `carried forward`, `metadata only`, `inputs changed during run`), runs the run check, and reports "R12: finished, 14 outputs, 2 `carried forward`". A declared output that was not produced fails the run check and is listed. Bare `sbatch` inside a `tm run` command prints a warning; `sbatch --wait` is accepted.

### CUJ 3: An agent session drafts claims and text

*   **Given**: Registered tables with parse specs exist, and `R12` is approved (or its code is owner-only). agentbox is installed.
*   **When**: The owner runs `agentbox run --toulmin`, and agentbox calls `tm session start --role writer --json` (without agentbox, the owner runs `tm session start --unisolated --harness claude-code` and pastes the printed MCP configuration into the harness). Toulmin takes the lock, allocates `S4` and a token, writes a session-started event, and prints the start message: session number, token, bridge binary, socket directory, agent-denied list, and must-not-mount list. agentbox clones the repository onto a session branch, plans its mounts (the clone as project root, the socket directory at a container path of its choosing such as `/run/agentbox/toulmin/`, the bridge binary read-only), hashes the planned mount list, calls `tm session confirm S4 --json FILE` with its preset, isolation level, network mode, the container paths of the socket directory and bridge, and that hash, then starts the tool server outside the container with `tm session serve S4` (which refuses an unconfirmed session) the way it starts its other per-session services, and writes the bridge command into the harness's MCP configuration with the token in a `0600` file in the run directory, never in the environment. The agent, inside the sandbox, sends MCP over stdio to the bridge, which forwards to the tool server's Unix socket (host path `/tmp/toulmin-$UID/S4/sock`). The agent calls `query` (files, titles, runs, lineage, flags), `preview_steps` on `F7@v2`, `propose_claim` with sentence, cited versions, citations, query steps, assertion, and qualifiers, writes a sentence into `report.md` on its session branch in the mounted clone as `[Gene X is upregulated ...]{.claim ref=S4.12}`, calls `propose_title` for an untitled output, and `draft_declaration` for a follow-up analysis. At the end, the launcher writes the end message (working tree path, base and head commits) to a file and calls `tm session end S4 --json FILE`.
*   **Then**: Each proposal is one event in journal `S4`, numbered by the tool server (`S4.1`, `S4.2`, ...). Each `propose_claim` response carries the system verdict (evidence exists; assertion holds or fails) and the claim's current flags, so the agent sees a failing assertion at once. A claim citing an unapproved code version carries that flag in the response. Refusals are typed: a host path instead of a file handle returns the rule, the bad field, and an empty `owner_command`, because no owner command can make a host path valid; a declaration with a path outside the project is refused. Every call and the result hashes go to the session's diagnostic log outside the record. At session end, Toulmin computes `git patch-id --stable` for each commit between the reported base and head in the clone and records them. `tm status` shows "S4 ended, 9 claims proposed, 2 failing, 1 declaration drafted, 3 agent commits".

### CUJ 4: Review with the analysis tree, approve, promote, and send back

*   **Given**: `S4` has ended. The agent's session branch is merged (git, outside Toulmin).
*   **When**: The owner runs `tm audit` (CUJ 8), then `tm view --out review/` and opens `review/claims/S4.12.html`, or `tm view --serve` for the live viewer. On the page they see the sentence, the cited rows highlighted in their full rows with neighbors, each query step's output and plain-language description, the assertion and its verdict, the qualifiers, "other claims citing these rows", and the analysis tree: `F7@v2` produced by `R12` of `A3` (code version, approval state, diff since the last approved version with agent lines marked, run check, staleness) from `data/counts.tsv@v1` and `config/de.yaml@v1`, back to the registered inputs. They tick "approve code version", "confirm", or "promote" on each item; the page's script gathers the ticks into an action file that the browser offers as a download, and the owner saves it as `review/actions.json`; they run `tm apply review/actions.json`, which prints one line per item with its target, revision, and any override, and asks for confirmation. In the live viewer the same actions go through a one-time confirmation code plus the per-launch token. For `S4.7` they choose "send back" with a note.
*   **Then**: `run.approved` events record the approval of the code version, scoped to script fingerprints plus command template. `draft.promoted` events on the main chain point at the exact journal revision events accepted, with `--allow-unapproved` recorded if used; a selection missing a dependency (a claim whose cited version is not registered) is refused with the missing item named. Promoted claims carry `promoted, not reviewed` until a review exists; `--confirm` records a confirmed review at the same time. The first promotion citing `F7@v2` copies its bytes into the kept-files folder, compressed and content-addressed, after a size warning above the threshold. `S4.7` is marked rejected with the note; it stays in journal `S4`, and the next session's queries list it under open rejections. Actions confirmed in the live viewer land in that launch's journal and are shown as `pending viewer action` until the owner commits them with `tm apply --viewer` or confirms them in `tm status`.

### CUJ 5: Correct a claim

*   **Given**: `S4.12` is promoted. The owner finds (a) a typo in its sentence, (b) a claim `C9` that is wrong, and (c) a claim `S4.3` that should be replaced by a more careful one.
*   **When**: (a) The owner edits the sentence in `report.md`, sees `text drifted` in `tm status`, and runs `tm claim update-text S4.12`. (b) The owner runs `tm claim retract C9 --reason "wrong contrast"`. (c) The owner runs `tm claim supersede S4.3 --sentence ... --steps steps.json --cited F7@v2` (or promotes an agent's `propose_supersede`).
*   **Then**: (a) A new revision event keeps ref `S4.12`, records the edited sentence, re-runs both checks, and clears the drift flag; the earlier sentence stays visible in the claim's history. (b) `C9` stays visible with the `retracted` label and the reason; its spans in the report get the `points at a retracted claim` flag with the drift flag. (c) A new claim `C32` is created, pointing back at `S4.3`; `S4.3` gets the `superseded` label; spans still carrying `ref=S4.3` get `points at a superseded claim` (replacement `C32`) until the owner edits them. Nothing is overwritten.

### CUJ 6: A human writer adds a claim

*   **Given**: Registered table `F7@v2` with a parse spec. `report.md` is registered with `tm report add report.md`.
*   **When**: The owner writes `steps.json` (filter `padj < 0.05`, sort by `-log2FoldChange` with missing values last, take 5, with the key as tie-breaker) and an assertion (`rank of gene_id = "GeneX" equals 5`), writes `[Gene X has the fifth highest fold change]{.claim ref=C31}` in the report, and runs `tm claim add --report report.md --ref C31 --sentence "..." --cited F7@v2 --steps steps.json --assert assert.json --kind quantitative`. For a qualitative claim they omit steps and assertion and cite a whole file, page, or panel.
*   **Then**: The CLI allocates `C31` under the lock, writes the claim straight into the main chain (owner claims need no promotion), runs the evidence-exists check and the assertion, stores the verdict with the generated SQL, parse-spec fingerprint, translator version, DuckDB version, loaded extensions, and rule version, and prints the verdict and the per-step row counts. A missing required choice (no tie rule on a rank) is refused with the field named. The span scanner finds `ref=C31` in `report.md` and records the sentence; a later edit flags drift as in CUJ 5.

### CUJ 7: An upstream script changes, staleness, a rerun, and accepting claims onto a new version

*   **Given**: A chain `A1` (counts) -> `A3` (DE) -> `A5` (plots). Claims cite outputs of `A3` and `A5`.
*   **When**: The owner edits `scripts/counts.R` and runs `tm status`. Then they run `tm run again R10` for `A1`; Toulmin prints a resubmit line because `R10` ran in a batch job. After the job, `tm status` again, then `tm run again` for `A3` and `A5` in the listed order. Finally `tm claim accept --version F7@v3 --all`.
*   **Then**: After the edit, `A1` is `own code changed`, and `A3` and `A5` are `upstream changed`, listed in dependency order with their recorded commands; claims citing their outputs inherit the flag with the same reason. After `A1` reruns with byte-identical output (or a table identical after sorting by key, or identical decompressed content), the spread stops: `A3` and `A5` lose the flag without rerunning. If `A1`'s output differs, `A3`'s rerun produces `F7@v3`; quantitative claims whose steps still run and whose keys are still found are re-checked against `v3` with new verdicts, while other claims get `evidence moved` (newer version exists). No claim changes its cited version until `tm claim accept`, which moves the selected claims to `v3` in bulk and records the accept event. A hand edit to `config/de.yaml` makes a new input version automatically, labeled `changed outside a run`, and makes `A3` `own inputs changed`. An output that `A3` no longer declares is retired, and claims citing it get `cited file retired`; `tm file restore` brings it back if the removal was a mistake.

### CUJ 8: `tm audit` before review

*   **Given**: A project with kept files, several journals, receipts, and verdicts from earlier weeks. The owner is about to review `S4`.
*   **When**: The owner runs `tm audit` (or `tm audit --since LAST`).
*   **Then**: Toulmin re-hashes every tracked file and kept copy, verifies the main chain and every journal (reporting torn tail, fork, and corruption separately), checks each recorded journal head against the journal, re-runs every claim check and compares with the stored verdict, and lists out-of-date analyses, unapproved code versions, open flags, unpromoted drafts, pending and quarantined receipts, and pending viewer actions. It writes `audit.json` and `audit.html` explaining each hash mismatch (last confirmed time, dependents) and each verdict change (data changed, DuckDB version changed, or checker version changed). A run on an unchanged project reports zero differences.

## Technical Plan (Architecture)

### 1. Glossary

One meaning per word. Where the earlier documents used several words for one thing, the first column is the only word this document uses.

| Term | Meaning |
| --- | --- |
| Project | One git repository plus one record directory. `tm init` binds them. |
| Record directory | The project's permanent store on disk: main chain, journals, inbox, and kept files. Lives outside the repository. |
| Record (the) | The owner-accepted content: the main chain plus the journal events a promote event names. |
| Record library | The `tm-record` crate, which reads and writes the record directory. |
| Handle | A short readable identifier: `F7`, `R12`, `A3`, `S4`, `V2`, `F7@v2`. Not an ID. |
| Fingerprint | A `sha256:` digest of a file's bytes or of a directory manifest. "Hash" is used only for events and IDs. |
| Lineage | The graph of edges from output versions to the input versions a run used. |
| Analysis tree | The lineage of one cited version rendered back to registered inputs, with runs, code versions, and approvals. |
| Proposal | A journal event by a session. A session's draft is the set of its unpromoted proposals. |
| Main chain | The owner's append-only event history, written only by the CLI, in segments. |
| Segment | One file of the main chain. Sealed segments are packed. |
| Journal | One writer's append-only event file with its own hash chain: an agent session (`S4`) or a viewer launch (`V2`). |
| Receipt | A self-contained JSON file written by the run wrapper into the inbox: "started" or "finished". |
| Inbox | The folder where receipts arrive; one in the record directory and a fallback in the project. |
| Index | A rebuildable SQLite cache built from the main chain and open journals. Never the source of truth. |
| Read-in | The CLI reading new journal lines and receipts into the main chain, under the lock. |
| Event | One JSON object in a chain or journal, with an envelope and a body. |
| Analysis | A declared recipe: command template, scripts, support scripts, inputs, outputs, and params. |
| Run | One execution of an analysis, created from receipts. |
| Run group | One run with one part per job-array task. |
| Code version | The fingerprints of an analysis's declared script and support scripts plus its command template. The unit of approval. |
| Run check | The check applied to a run after its receipts arrive: declared outputs produced, snapshots compared, inputs unchanged. |
| Claim check | The two system checks on a claim: evidence exists, and assertion. |
| Disk scan | The stat-only pass behind `tm status`, session start, and the viewer. |
| Verify | The chain, journal, and kept-file integrity part of `tm audit`. |
| Query steps | The JSON operations a claim applies to one or more tables. |
| Assertion | The versioned comparison a quantitative claim makes on the final rows. |
| Citation | A pointer from a claim into a file version: cells by key, whole file, page, or panel. |
| Claim | A sentence, its kind, cited versions, citations, query steps, assertion, and qualifiers. |
| Ref | A claim's readable handle: `S4.12` or `C31`. Never changes across revisions. |
| ID | An event's or object's permanent identity: SHA-256 of its canonical JSON. A claim's ID is the ID of the event that first created it: `claim.proposed` for an agent claim, `claim.added` for an owner claim. |
| Revision | A new event that keeps a claim's ref and changes its content. |
| Supersede | A new claim with a new ref that points back at the old one. |
| Retract | An owner event withdrawing a claim with a reason. The claim stays visible. |
| Review | A human line of evidence on a claim or run: confirmed, disputed, or question, with a note. Never overrides a check. |
| Verdict | The recorded result of one system check: holds, fails, not checkable, or error, with version information. |
| Flag | System-raised attention with a named clearing action. |
| Label | A permanent attribute. Labels are never cleared. |
| Staleness | An analysis's state when its code, command, inputs, or anything upstream changed since its latest successful run. "Out of date" is the plain-English form of the same state. |
| Promote | An owner event moving selected journal events into the record. |
| Send back | An owner event marking draft items rejected, with a note. |
| Accept | An owner event moving claims onto a new file version. |
| Approve | An owner event accepting a code version. |
| Keep | Copy a file version's bytes into the record directory. |
| Mark removable | An owner event saying the on-disk file may be deleted. |
| Retire | A file whose analysis no longer declares it. Automatic. |
| Restore | Bring a retired file back. Owner only. |
| Agent-denied files | Files the agent tool server refuses to serve, and which the launcher must not mount. |
| Session | One agent working period with a number, a token, a journal, and an isolation level. |
| Owner capability | A Rust value required by every owner operation and by every main-chain append; constructible only in `tm-cli`. |

#### 1.1 Toulmin's argument model mapped to system terms

| Toulmin term | System term | For a quantitative claim | For a qualitative or image claim |
| --- | --- | --- | --- |
| Claim | Claim sentence | The sentence and its assertion | The sentence |
| Grounds | Cited file versions and citations | The cells the query steps produce, mapped back to input keys | The whole file, page, or panel |
| Warrant | Query steps and assertion | The declared steps and the comparison rule that make the cells support the sentence | None in V1; the warrant is the reviewer's expertise, and the claim is shown that way |
| Backing | Analysis tree | Runs, code versions, approvals, and registered inputs behind each cited version | The same |
| Qualifier | Qualifiers field | Free text recorded with the claim ("in this dataset", "likely") | The same |
| Rebuttal | Reviews and flags | Disputed reviews, failing re-checks on new versions, and "other claims citing these rows" | Disputed reviews and flags |

### 2. Process and deployment

#### 2.1 Processes

| Process | Started by | Lives | Writes | Reads | Network |
| --- | --- | --- | --- | --- | --- |
| `tm` CLI | The owner, in a shell | One command | Main chain (under the lock), index, stat cache | Everything | None, except `tm view --serve` (loopback) |
| Run wrapper (`tm run`) | The owner's shell or batch script, on any node | One command's duration | Receipts in the inbox | Declarations, recorded hashes, stat cache | None |
| Agent tool server (`tm-agent`) | The launcher (agentbox) or the owner, via `tm session serve S<n>`, which execs `tm-agent` in the foreground; the launcher backgrounds and restarts it within a session; outside the sandbox | The session | Journal `S<n>`, the session's diagnostic log, parse cache | Index, main chain, journals, registered file bytes, parse cache | Unix socket at `/tmp/toulmin-$UID/S<n>/sock`; the directory `S<n>/` is the only path under `/tmp/toulmin-$UID/` mounted into the sandbox |
| stdio bridge | The agent harness, inside the sandbox | The harness | Nothing | Nothing | stdio to the harness, Unix socket to the tool server |
| Live viewer (`tm view --serve`) | The owner | Until closed | Journal `V<n>`; its token file | Index, main chain, journals, kept files, working tree | Loopback HTTP with a per-launch token |
| Static view (`tm view --out DIR`) | The owner | One command | Pages in `DIR`; an action file via the owner's browser download | The same as the viewer | None |

There is no process that outlives a command except the tool server and the live viewer, and both exit when their session or launch ends. Nothing runs as a service, nothing needs root, and nothing needs a fixed port; the live viewer binds an ephemeral loopback port.

#### 2.2 Three setups

**Setup 1: workstation with agentbox.** Everything is on one machine. The record is in `~/toulmin-records/<project>/`. agentbox runs the agent in a rootless container with a per-session `git clone` of the repository mounted as the project root (`projects/<name>/clones/S<n>/`, on a session branch; the owner's own checkout, including raw data and uncommitted work under it, is not mounted), the record directory and the other must-not-mount paths absent, and the bridge binary mounted read-only. The agent commits on the session branch in the clone; at session end agentbox reports the clone path, base, and head, and keeps the clone until the owner fetches the branch and removes it. A worktree would not work: its `.git` is a file pointing into the owner's repository, which is not mounted. The tool server runs as the owner outside the container. Isolation level recorded: `container`, with the network mode the launcher reports (`locked`, `packages`, or `open`).

**Setup 2: shared cluster with agentbox on the login node.** The record is on a shared filesystem that compute nodes can see (set at `tm init`, checked by `tm doctor`). Agents run in rootless containers on the login node (or on a dedicated interactive node) via agentbox. Runs happen inside batch jobs on compute nodes, which write receipts into the record's inbox, or into the project's fallback inbox if they cannot see the record. The owner's next CLI command on any login node reads them in. The index and stat cache live on local disk per login node and rebuild themselves when their recorded chain head is behind.

**Setup 3: no agentbox (unisolated).** The agent harness runs as the owner. `tm session start --unisolated --harness NAME` allocates the session, records the confirm message itself (`isolation: unisolated`, `preset: none`), and prints the MCP configuration for the bridge and the command `tm session serve S<n>`, which the owner runs in a second terminal or in the background. The session is labeled `unisolated`. The agent could read or write anything the owner can; Toulmin records the level as `unisolated`, warns in `tm status` when owner actions happen while an unisolated session is open, and asks for confirmation on owner actions when a known harness environment variable is set.

#### 2.3 The batch-job sequence

```text
login node                         compute node                      record (shared fs)
----------                         ------------                      ------------------
owner: sbatch run_de.sh
                                   job starts
                                   tm run A3 --param threads=16
                                     fingerprint scripts, git commit, diff
                                     snapshot deps and outs (stat)
                                     write inbox/tmp/<id>.started.json
                                     rename -> inbox/new/           ------> started receipt
                                     exec: Rscript scripts/de.R 16
                                     ... hours ...
                                     (SIGTERM at limit: partial receipt,
                                      exit; tm run finish later)
                                     snapshot again; hash new or changed
                                     write inbox/tmp/<id>.finished.json
                                     rename -> inbox/new/           ------> finished receipt
                                   job ends
owner: tm status
  take lock (O_EXCL, epoch check)
  read inbox/new/*.json          <--------------------------------------- receipts
  Files: version outputs, labels
  Runs: create R12, run check
  append batch to main chain     ---------------------------------------> main chain
  move receipts to inbox/cur/
  release lock
  print status
```

Rules the sequence relies on: the wrapper never takes the lock; receipts are named by their content hash and read in idempotently; a `finished` receipt without a matching `started` receipt is read in with the label `started receipt missing`; a `started` receipt whose `node.started` plus the late-output wait (setting `run.late_output_wait`, default 10 minutes) has passed by the clock of the node that reads it in, with no `finished` receipt, shows as `run in progress` and then `interrupted` (this one comparison is clock-dependent, and the flag reason says so); the record directory must support atomic exclusive create and atomic rename, which `tm doctor` tests on the record path, refusing filesystems that fail (NFSv2, some CIFS and FUSE mounts).

### 3. Writer model

Every writer owns its own file (section 2 gives the processes).

| Writer | Writes to | Concurrency rule | Hash chain |
| --- | --- | --- | --- |
| Owner CLI | Main chain segments and `HEAD` | Short `O_EXCL` lock file with an epoch number; fencing before every write | Yes, across all segments |
| Agent tool server | `journals/S<n>.jsonl` | None needed; the session number is allocated under the lock, so there is one writer | Yes, per journal |
| Live viewer | `journals/V<n>.jsonl` | None needed; the launch number is allocated under the lock, so there is one writer (D26) | Yes, per journal |
| Run wrapper | `inbox/new/<hash>.json` | Write-then-rename; no two receipts share a name | No; receipts are single records hashed by content |

#### 3.1 Main chain

- **Lock.** `main/lock` is created with `O_EXCL`. It holds host, process ID, start time, a heartbeat time refreshed every 5 seconds (setting `lock.heartbeat_seconds`) by the CLI while it holds the lock, and an epoch number. The heartbeat is refreshed by rewriting the lock file in place, never by rename, so the inode created with `O_EXCL` is the one every reader checks. A command waiting for the lock prints the holder's host, process ID, and the time until takeover is allowed.
- **Epoch and fencing.** `HEAD` records the epoch of the last successful batch. Taking the lock increments the epoch. Taking the lock, or breaking it, writes the new epoch to `HEAD` by write-then-rename before anything else. Before every batch append, the CLI re-reads `HEAD` with the attribute cache bypassed (a fresh open after an `opendir` and `closedir` of `main/`, which most NFS clients treat as a directory-cache invalidation; the length check below is the protection that does not depend on client behaviour). It refuses to write if `HEAD.epoch` is higher than its own, or if the open segment's length differs from `HEAD.offset`. A paused writer that woke after a takeover therefore cannot fork the chain, and a mismatch is reported as a fork rather than written over. `tm doctor` probes rename visibility across two processes on the record path, and `tests/lock.rs` includes an NFS-mount case.
- **Takeover.** `tm lock break` (or an automatic takeover during any locked command) is allowed only when the heartbeat has not changed for the takeover timeout (OQ2), measured on the local monotonic clock between two reads. It increments the epoch and records a `lock.taken_over` event.
- **Read-in.** Every CLI command that takes the lock first reads new journal lines and new receipts, runs the libraries' read-in logic, and appends the results plus a `journal.head_recorded` event per journal that grew. Within a batch, read-in events precede the command's own events, so `tm session end` reads in with the session still open. `tm run` never takes the lock and never reads in. Commands that only read (for example `tm file show`) do not take the lock and serve from the index plus open journals; `tm status` takes the lock because it reads in.
- **Torn tail.** A batch that lies beyond `HEAD.offset` and whose declared length is not satisfied or whose hash does not match is a torn tail. Readers stop before it. The next locked command truncates it after writing a `chain.torn_tail_truncated` event in the new batch, so the fact is on the chain. Anything inside `HEAD`'s range that fails verification is corruption, not a torn tail: `tm audit` reports it and nothing truncates it.

#### 3.2 Journals

- One event per line, each carrying the previous line's hash. Every line is synced before the writer replies to its caller. A torn last line (no trailing newline, or invalid JSON) is skipped by readers.
- A restarting tool server (same session, after a crash) scans its own journal to recover the chain head and its claim counter, truncates a torn last line, records `journal.torn_tail_truncated` as its first new line, and removes and recreates its socket file.
- Journals are permanent. Unpromoted and sent-back drafts stay in them.
- A journal's events become part of the record when a promote event on the main chain names them. Before that, they are visible through the index as proposals.
- A viewer launch journal holds reviews and confirmed owner actions. Reviews are read in as reviews. A confirmed owner action is read in as the corresponding main-chain event with `source: viewer` and the hash of the viewer event that confirmed it (D27). A `viewer.action_confirmed` event records the chain head and the exact target revision IDs at confirmation time; at read-in the Core applies the same checks as `tm apply` (`E8101` outdated target, dependency closure, approval gate) plus a check that the launch was still open, and a failing action is read in as `viewer.action_rejected` with the reason, never applied. Until read in, the action is pending; the viewer shows this, `tm status` lists pending viewer actions and commits them only after an interactive confirmation, and `tm apply --viewer` commits them without one.

#### 3.3 Receipts

- Written to `inbox/tmp/`, renamed into `inbox/new/`. Named `<sha256 of content>.json`. Read-in keyed by that hash is idempotent.
- Malformed receipts move to `inbox/quarantine/` with a reason file. Read-in receipts move to `inbox/cur/`.
- A fallback inbox at `<project>/.toulmin/inbox/` serves compute nodes that cannot see the record. `tm run ingest PATH` reads from any path. A receipt read from the project inbox is labeled `possibly agent-issued` when any session was open at any instant between the receipt file's ctime in `inbox/new/` (set by the rename, not by the receipt's self-reported `node.started`) and the read-in, plus a grace window (setting `session.receipt_grace`, default the late-output wait); the label carries to the run, its output versions, and claims citing them (D28).

#### 3.4 Index and stat cache

One SQLite file per machine, on node-local disk: `$XDG_CACHE_HOME/toulmin/<project-id>/<hostname>/index.sqlite` when `$XDG_CACHE_HOME` is set and on a local filesystem (as judged by `tm doctor`), otherwise `$TMPDIR/toulmin-$UID/<project-id>/`, then `/tmp/toulmin-$UID/<project-id>/`. `tm doctor` refuses a network filesystem for the index and the stat cache. The index records the main-chain head and each journal's last line ID it was built from. On open it runs `PRAGMA integrity_check`; a failure, or a recorded head that is not on the chain, triggers a rebuild. Any command that finds the head behind applies the new events through each library's fold; a full rebuild must finish in under 60 seconds for one million events. The index is on the must-not-mount list.

A stat cache is the `stat_cache` table inside the index database described above, defined in 11.17 and excluded from rebuild drops, holding, per tracked path, the last snapshot and the fingerprint it was confirmed against. The wrapper on a compute node does not read a login node's cache: at each read-in the CLI writes a derived file per analysis, `analyses/<handle>/latest.json` in the record directory (rebuildable, not part of the chain), holding the latest recorded snapshot and fingerprint for every declared path; the wrapper reads that one file and writes its own snapshots into the receipt.

### 4. Modules and boundaries

#### 4.1 Layout

| Part | Kind | Crate | One line |
| --- | --- | --- | --- |
| Shared formats | Shared code, pure | `tm-formats` | Event envelope, IDs, canonical JSON, typed values, citations, query step operations, assertion schema, receipts, declarations, handshake messages, and error codes |
| Filesystem facts | Shared code, pure, no state | `tm-fsfacts` | Stat snapshots, hashing, directory manifests, exclusions, and the racy rule |
| DuckDB engine | Shared code, no state | `tm-duckdb` | One function that opens an in-memory DuckDB with the 11.6 settings, registers Arrow tables, applies the per-call time limit, and reports the runtime version and loaded extensions; the only crate with `unsafe` (the binding) |
| Index transaction | Shared code, no state | `tm-index` | The `IndexTx` type over one `rusqlite::Transaction`, the `Row` and parameter types the folds use, and the `rusqlite` dependency; every library's fold and the Core write through it |
| Record library | Library | `tm-record` | Main-chain segments, journals, inbox, lock and epochs, kept-files folder, event scans, and integrity checks |
| Files | Library | `tm-files` | File identity, versions, attributes, parse specs and keys, and the parse cache |
| Runs | Library | `tm-runs` | Declarations, receipts, run checks, lineage per version, approval scope, and attribution matching |
| Checker | Library | `tm-checker` | Expression evaluation, query steps to explicit DuckDB SQL, assertions, general rules, and result hashes |
| Claims | Library | `tm-claims` | Claims, refs, revisions, verdicts, reviews, flags, new-version re-checks, report-span scanning, and drift |
| Owner capability | Tiny crate | `tm-owner-cap` | The `OwnerCap` type (private field, no public constructor) and, behind the cargo feature `construct`, its only constructor; `tm-cli` enables the feature and no other crate does |
| Core | Application layer | `tm-core` | Every user-level operation, all permission checks, staleness, the read interface, journal and receipt read-in, and index folds |
| CLI | Front end | `tm-cli` (binary `tm`) | Owner commands, the run wrapper, `tm apply`, `tm view --out`, and `tm view --serve` |
| Agent tool server | Front end | `tm-agent` (its own static binary; does not depend on `tm-cli`) | One per session; MCP over a Unix socket; started by `tm session serve`, which execs it |
| stdio bridge | Front end | `tm-bridge` | Forwards MCP over stdio to the socket; the only binary that runs inside the sandbox |
| Live viewer | Front end | `tm-viewer` (linked into `tm`) | Read interface, reviews, and owner actions through a confirmation step |
| Static view | Output of the Core | part of `tm-core` | Pages plus action-file export |

The five libraries, both shared crates, and the Core are synchronous. Async code exists only in `tm-agent` and `tm-viewer`.

```text
            +-----------+   +-----------+   +-----------+
            |  tm CLI   |   | tm-agent  |   | tm-viewer |
            | (owner)   |   | (session) |   | (owner)   |
            +-----+-----+   +-----+-----+   +-----+-----+
                  |  OwnerCap      |  session token   |  per-launch token + one-time code
                  v               v                  v
            +-------------------------------------------------+
            |                    tm-core                      |
            |  operations table, permissions, staleness,      |
            |  read interface, read-in, index folds           |
            +---+--------+--------+--------+--------+---------+
                |        |        |        |        |
                v        v        v        v        v
            Record    Files    Runs    Checker   Claims      (never call each other)
                ^        ^        ^
                |        |        |
            tm-formats, tm-fsfacts, and tm-duckdb (shared, used by anyone)
```

#### 4.2 Rules between parts

1. **Libraries never call each other.** The Core passes data between them: a parsed Arrow table from Files into the Checker, Checker results into Claims, file version handles from Files into Runs.
2. **Shared formats, filesystem facts, the DuckDB engine crate, and the index-transaction crate** can be used by any part. They hold no state and never touch the record.
3. **Front ends only call the Core.**
4. **Permissions are checked only in the Core.** Libraries receive plain, already-authorized decisions. Owner operations take an `OwnerCap` argument, and so does the Record's main-chain append, so `read_in` and system re-checks pass through the same gate (the CLI holds the capability for the whole command). Every crate may name the type; only `tm-cli` may construct it. `tm-agent` and `tm-bridge` are separate binaries that do not depend on `tm-cli`, so no process other than `tm` ever holds an `OwnerCap`. CI builds `tm-agent` and `tm-bridge` standalone with the `construct` feature off and fails if any crate other than `tm-cli` names `OwnerCap::new_for_cli`. This is defence in depth: the primary controls are that `tm-agent` exposes no owner tool and that the record directory is not mounted in the sandbox.
5. **Who writes what.** Only the CLI writes the main chain, under the lock. Tool servers and viewer launches append only to their own journals. The run wrapper writes only receipts.
6. **Index ownership.** Each library supplies its own fold from its events to index tables (`fn fold(&mut IndexTx, &Event)`), and the Core runs the folds. The Record library stores nothing derived.
7. **Async stays at the edge.** `tm-agent` and `tm-viewer` call the Core from blocking tasks; the Core never awaits.

#### 4.3 Event ownership

| Events | Owner | Written to |
| --- | --- | --- |
| Project created, settings changed, lock taken over, journal head recorded, receipt read in, event quarantined, torn tail truncated | Core | Main chain |
| Session started, confirmed, ended, closed, abandoned, commits recorded; viewer launch started, ended, abandoned; viewer action rejected; promote (with any override); send back; owner action via viewer or `tm apply` (as the resulting event, with its source); audit completed | Core | Main chain |
| File tracked, registered, version added (including `changed outside a run`), kept, marked removable, retired, restored, renamed; parse spec set; panels set; provider checksum checked | Files | Main chain |
| Analysis declared; run started and finished (from receipts); run checked; approval (acceptance of a code version) | Runs | Main chain |
| Claim proposed or revised; title and description proposed; analysis declaration drafted; supersede proposed; accept proposed; draft note | Claims or Runs, by subject | Session journal |
| Claim added by owner, checked, reviewed, accepted onto a new version, text updated, retracted, superseded; report registered | Claims | Main chain |
| Review from the viewer; confirmed owner action from the viewer | Claims (review), Core (action) | Viewer launch journal |
| DuckDB or checker upgrade, with the changed-verdict report | Core | Main chain |

Computed, not stored: staleness, cited and uncited counts, agent attribution, "other claims citing these rows", and the current revision of a claim.

#### 4.4 What crosses each boundary

| Boundary | In | Out |
| --- | --- | --- |
| Shared formats | Nothing stateful | Types and pure functions: canonical JSON, ID derivation, schema validation, error codes |
| Filesystem facts | Paths, exclude lists, settings, previous stat values | Snapshots, hashes, manifests, "needs re-hash" results |
| DuckDB engine | Settings and Arrow tables | An open connection with the settings applied; version and extension facts |
| Index transaction | SQL and parameters from a fold | Rows, and the recorded chain head |
| Record library | From the Core only: a framed batch for the main chain; kept-file paths with expected hashes (the Record library streams, compresses, and checks them). Journals appended by their own writers through the journal API | Event scans (all, by type, after a position), journal reads, inbox listings, kept bytes, integrity results (torn tail, fork, corruption) |
| Files | Filesystem facts, declarations' expanded paths from the Core, receipts' output facts, owner decisions passed by the Core | Identities, versions, attributes; Arrow tables from the parse cache; page counts (PDF) and registered panel labels; content-equivalence results (same bytes, same table by key, same decompressed content) |
| Runs | Declarations, receipts, git facts (commit, script diff, commit list with patch IDs), session reports, file version handles from Files | Run records and run-check results; lineage edges between versions; code-version identities for approval; matched agent commits |
| Checker | A named set of Arrow tables with parse-spec fingerprints, the query steps, the assertion, the rule version, and DuckDB settings | Per-step outputs and counts; plain-language descriptions; final rows mapped to input keys; the assertion result; the generated SQL; the result hash; all version information |
| Claims | Proposals; Checker results; Files lookups (key present, page count, panel labels, availability); staleness, approval state, and version status per cited version from the Core; report file text from the working tree; reviews | Claims, refs, revisions, verdicts, reviews, flags with reasons and clearing actions, drift results |
| Core | Requests from front ends, each carrying an actor (owner, owner via viewer, or session) | Results and typed errors |
| Agent tool server | MCP calls through the bridge, authenticated by the session token | Core requests made as that session, results carrying flags; each call logged to the session diagnostic log |
| Live viewer | Owner browser requests with the per-launch token | Pages from the read interface; the analysis tree per claim; reviews and confirmed owner actions into journal `V<n>` |
| Static view | The read interface | Pages; `actions.json` |

| External system | Direction | What crosses | Notes |
| --- | --- | --- | --- |
| git | Toulmin reads | Commits, script diffs, patch IDs, report files in the working tree | Never commits or changes branches; `--no-optional-locks`; only declared scripts' paths |
| agentbox | Both ways | Versioned handshake: start (session number, token, bridge path, lists) and end (clone path, base and head commits) | agentbox is a separate project by the same author that launches agents in rootless podman containers (`--userns=keep-id`, so the agent runs as the owner's UID) with a per-session clone of the repository mounted; unisolated sessions use the same messages without it; Toulmin trusts the launcher's report |
| Agent harness | Both ways | MCP over stdio to the bridge, then the Unix socket | Agent-denied files are refused; registered file handles only |
| Scheduler | Toulmin reads | Environment variables matching `SLURM_`, `PBS_`, `SGE_`, `LSB_` | Never submits or queries jobs; catches `SIGTERM` |
| Workflow tools | Wrapped | Head-process command, exit status, declared files | One run per head process in V1 |
| Filesystem | Both ways | Repository (stat and hash), record directory | `tm doctor` checks reachability, type, and mount options |
| Browser | Both ways | Static pages; live pages, reviews, confirmed actions | Static pages: the same Markdown sanitizer as the viewer, no inline event handlers, no external resources. Live pages: token, Host, and Origin checks, sanitized Markdown, strict content security policy |
| DuckDB | Embedded | Explicit SQL, settings, version, extensions | Pinned exactly; no auto-install or auto-load; Parquet compiled in |
| Cloud model providers | None | Nothing | Toulmin never talks to a model; the harness does. The agent-denied list and the "outside content is data" marking are the controls |

### 5. Trust model

#### 5.1 Actors

| Actor | Trusted for | Not trusted for | How Toulmin knows who it is |
| --- | --- | --- | --- |
| Owner | Approvals, promotions, accepts, opt-ins, settings | Scientific correctness | Runs `tm`, the only binary that constructs an `OwnerCap`; interactive-terminal and harness-variable checks label doubtful cases |
| Owner via viewer | The same, through a confirmation code | The same | Per-launch token plus one-time code at confirmation; re-validated at read-in; events carry `source: viewer` |
| Agent session | Nothing; its output is labeled as its own | Anything privileged | Session token on the socket; everything lands in journal `S<n>` |
| Launcher (agentbox) | Its report of preset, isolation, network mode, mounts applied, and the working tree's commits between base and head | Nothing else | The handshake |
| Run wrapper | Its receipt, as a statement of what it observed | Who invoked it | Receipt origin (record inbox or project inbox) and open sessions at read-in |
| Outside content (web page, PDF, downloaded file) | Nothing; data, never instructions | Instructions of any kind | Marked as data in tool responses |

#### 5.2 Isolation guarantees

| Isolation level (as reported by the launcher) | Agent cannot | Agent can | Toulmin's guarantee |
| --- | --- | --- | --- |
| `container`, network `locked` | Read or write the record directory, index, stat cache, session diagnostic logs, viewer tokens, other sessions' sockets and journals; reach anything but the model API through the session's proxy | Read and write the mounted session clone; call the tool server through the bridge; call its model | Every change to the record by this session came through the tool server and is in journal `S<n>`. This is the tightest level agentbox offers; there is no network-off mode |
| `container`, network `packages` or `open` | The same, except it can reach package registries, or the open network | The same, plus that network | The same for the record; outside content the agent fetched is not visible to Toulmin |
| `separate-user` (V1.x; launcher runs the agent as another Unix user) | Read paths the owner's permissions protect | Whatever that user can | Deferred with `separate-host`; listed so the handshake has a slot. It needs per-session socket directory and socket modes that admit one other user, which V1 does not specify |
| `unisolated` | Nothing | Anything the owner can, including `tm` owner commands and editing the record directory | None. Work is labeled `unisolated`; owner actions during the session are labeled `during unisolated session`, and `possibly agent-issued` when the harness variable is set; `tm status` warns |
| `separate-host` (V1.x) | Reach the service host without the forwarded socket | Everything on its own host | Deferred; listed so the handshake has a slot for it |

The hash chains add the same guarantee at every level: accidental damage, a partial edit, a torn tail, and a fork are detected by `tm audit`. A complete rewrite with recomputed hashes is not, at any level, because there is no outside anchor in V1.

#### 5.3 Threat table

| Threat | Attacker position | V1 control | Detect or prevent | Remaining risk |
| --- | --- | --- | --- | --- |
| Agent edits a script until a claim passes | Session, any level | Code versions with agent commits require approval; diff since last approved version marks agent lines; promotion refuses claims on unapproved code without `--allow-unapproved` | Visible at review | Owner approves without reading |
| Agent hand-edits a table it cites | Session, any level | Inputs changed outside a run become a new version labeled so; claims cite pinned versions; `tm status` flags | Detect | Agent edits before the file is first tracked |
| Agent loosens an assertion | Session | Revisions keep the ref and are separate events; the claim history shows each assertion | Visible at review | Owner does not open the history |
| Agent proposes a claim citing a file it should not see | Session | Agent-denied list enforced by the tool server and the launcher; registered handles only, never host paths | Prevent (container), label (unisolated) | Launcher applies the list wrongly; Toulmin trusts its report |
| Agent calls an owner action | Session, container | No owner tools exist in `tm-agent`; the record directory is not mounted; `OwnerCap` is unconstructible outside `tm-cli` (checked in CI) | Prevent | None within the stance for container sessions; unisolated sessions can run `tm` directly (next row) |
| Agent runs `tm` owner commands directly | Session, unisolated | Harness variable triggers confirmation and the `possibly agent-issued` label; `tm status` warns while the session is open | Label only | An agent with no telltale variable is not detected |
| Agent edits `.toulmin/project.json` or a project-side record pointer to redirect `tm` at a forged record | Session with a clone mounted | The record path is bound in `~/.config/toulmin/`, which is on the must-not-mount list; project IDs are cross-checked against `record.json`; the binding lists the repository roots it serves, so a `project.json` rewritten to another bound project's ID is refused | Prevent | None within the stance |
| Agent deletes or rewrites unkept inputs or scripts | Session with a clone mounted | The owner's checkout is not mounted; the clone holds only committed files, and raw data registered outside git is never in it; a hand edit to a tracked file becomes a new version labeled `changed outside a run` | Prevent for files outside git; detect for committed files | A committed data file rewritten on the session branch reaches the owner only through the merge |
| Agent forges a receipt | Unisolated session, or a clone whose `.toulmin/inbox/` is reachable | Project-inbox receipts read in during a session are labeled `possibly agent-issued` | Label | Record inbox is unreachable from a container, so forgery there needs unisolated access, which is labeled |
| Prompt injection via outside content | Any level | Content marked as data; no owner actions reachable | Prevent for owner actions | Agent drafts a misleading claim; the owner reviews it |
| Owner edits a cited input by hand and forgets | Owner | New version labeled `changed outside a run`; downstream staleness; claims flagged | Detect | None |
| Owner reruns one stage and forgets downstream | Owner | Staleness in dependency order with recorded commands; identical content stops the spread | Detect | Undeclared inputs are invisible (non-goal) |
| Two `tm` commands write at once | Owner, two terminals or nodes | `O_EXCL` lock, epoch fencing, segment length checked against `HEAD.offset` | Prevent on filesystems that pass `tm doctor`; otherwise detect (fork, by `tm audit`) | A writer that appends inside the attribute-cache window on a misconfigured mount produces a detectable fork, never silent loss |
| Paused writer wakes after takeover | Owner's paused process | Epoch written to `HEAD` first; epoch and offset check before every batch | Prevent on filesystems that pass `tm doctor`; otherwise detect | As above |
| Disk or filesystem corruption | Environment | Per-batch hash, chain links, synced appends, kept-file hashes | Detect | None |
| Index corruption, or an index behind the chain | Environment | Index records its head; integrity check on open; rebuild from the chain | Detect and rebuild | A corrupt index that passes the integrity check returns wrong rows until the next rebuild; owner operations never trust it (section 9) |
| Full rewrite of the record with recomputed hashes | Unisolated agent, or owner | None | Neither | Accepted under the stance; stated in the isolation table |
| Second viewer launch forks the viewer journal | Owner | One journal per launch (D26) | Prevent | None |

#### 5.4 Owner-only actions

| Action | Command | Event | What changes |
| --- | --- | --- | --- |
| Promote | `tm draft promote` | `draft.promoted` naming journal event IDs and the exact revision per claim | Named proposals become record items; first promotion citing a version keeps it |
| Send back | `tm draft send-back` | `draft.sent_back` with note | Items get the `rejected` label; visible to later sessions |
| Approve code version | `tm run approve` | `run.approved` scoped to code version | Lifts the promotion gate for outputs of that code version and its carry-overs; a rerun whose run check fails does not inherit the approval; approving when the analysis is out of date prints a warning |
| Accept claims onto a version | `tm claim accept` | `claim.accepted` per claim | Claims' cited version changes |
| Retract | `tm claim retract` | `claim.retracted` with reason | Claim gets the `retracted` label |
| Supersede | `tm claim supersede` | `claim.superseded` and a new `claim.added` | New ref created; old gets the `superseded` label |
| Update claim text | `tm claim update-text` | `claim.revised` by owner | Sentence recorded; checks re-run; drift cleared |
| Review | `tm claim review`, `tm run review`, viewer | `claim.reviewed`, `run.reviewed` | A review line; never changes a verdict; a claim review records the registered report's fingerprint at review time |
| Register, rename, keep, mark removable, restore | `tm file ...` | Files events | File attributes |
| Set parse spec | `tm table spec` | `file.parse_spec_set` | Table becomes citable by cells |
| Declare analysis | `tm analysis declare` | `analysis.declared` | New analysis or code version |
| Run, finish, `ingest`, run again | `tm run ...` | Receipts; `run.*` at read-in | New runs |
| Start, end, close sessions | `tm session ...` | `session.*` | Sessions and their journals |
| Settings and opt-ins | `tm settings set` | `settings.changed` | Recorded, shown to every reader |
| Apply an action file | `tm apply` | The actions' own events with `source: static-view` | As above |
| Upgrade DuckDB or checker | `tm upgrade` | `engine.upgraded` with changed-verdict report | Verdict versions |
| Break the lock | `tm lock break` | `lock.taken_over` | Epoch increments |

Agents have none of these. Static pages produce an action file and nothing else.

#### 5.5 Opt-ins and weaker options

| Option | Default | How enabled | Recorded where | Shown as |
| --- | --- | --- | --- | --- |
| Promote claims on unapproved code | Refused | `tm draft promote --allow-unapproved` | On the promote event | `promoted with unapproved code` on each claim |
| Unisolated session | Not available without `--unisolated` | `tm session start --unisolated --harness NAME` | On the session event | `unisolated` label on the session and its work |
| Approval required for all code versions (stricter) | Off | `tm settings set approval.require_all true` | Settings event | Owner-only code versions show `approval required` |
| Keep only the fingerprint of a large file | Keep bytes | Confirmation prompt at keep time, or `tm file keep --fingerprint-only` | On the keep event | `evidence not kept` on claims citing it |
| More hashing threads or a bandwidth cap in batch jobs | One thread, no cap | `tm settings set hashing.threads N`, `hashing.bandwidth_mib N` | Settings event; hashing time on receipts | None |
| Extra agent-harness variables | Built-in list | `tm settings set harness.env_vars ...` | Settings event | None |
| Custom default ignore list for directory manifests | `.snakemake/`, `.nextflow/`, `__pycache__/`, `*.swp`, `.DS_Store` | `tm settings set manifest.ignore ...` | Settings event, and the list on each declaration | None |
| Small-file hash threshold, limits, timeouts (OQ1 to OQ5) | As proposed | `tm settings set ...` | Settings event | None |

There is no auto-accept option for new versions in V1.

#### 5.6 Labels and flags

Labels are permanent. Flags have a clearing action.

| Name | Kind | On | Reason shown | Clearing action |
| --- | --- | --- | --- | --- |
| `declared` | Label | Run | Inputs and outputs come from the declaration; reads are not enforced | Permanent |
| `carried forward` | Label | Output version | Not rewritten by the run; content matches an earlier version | Permanent |
| `metadata only` | Label | Output version | Snapshot changed, content did not | Permanent |
| `content changed` | Label | Output version | New content | Permanent |
| `changed outside a run` | Label | Input version | Hand edit detected | Permanent |
| `inputs changed during run` | Label | Run | An input's snapshot changed between start and finish | Permanent |
| `started receipt missing` | Label | Run | Finished receipt arrived alone | Permanent |
| `declaration changed during run` | Label | Run and its output versions | The receipt's declaration differs from the current one at read-in | Permanent |
| `interrupted` | Flag | Run | Partial receipt, or started with no finish after the wait | `tm run finish`, or a new run |
| `run in progress` | Flag | Run | Started receipt, within the wait | Arrival of the finished receipt |
| `run check failed` | Flag | Run | Named reason: output not produced, exit status, inputs changed | A new run, or `tm run review --note` acknowledging it |
| `approval required` | Flag | Code version, or one rerun | Agent commits, or unattributed changes during an open session, or the strict setting, or a rerun whose run check failed | `tm run approve` |
| `owner code, not separately reviewed` | Label | Code version | Auto-approved | Permanent |
| `attribution partial` | Label | Code version | Unmatched commits, with a count | Permanent |
| `possibly changed (unconfirmed)` | Flag | File version | Stat changed on a file above the threshold during a disk scan | `tm status --refresh --hash`, or the next `tm run` |
| `retired` | Label | File identity | Analysis no longer declares it | Permanent; `tm file restore` adds a `restored` label (6.1 says how Retired is computed) |
| `restored` | Label | File identity | Owner restored a retired identity | Permanent |
| `removable` | Label | File identity | Owner marked it | Permanent |
| `evidence not kept` | Label | Claim | Owner kept only the fingerprint | Permanent |
| `expected-missing` | Label | File version | Marked removable and no longer on disk | Permanent |
| `root not declared` | Label | Lineage edge | The input version has no registered or declared origin | Permanent |
| `own code changed`, `own inputs changed`, `command changed`, `upstream changed` | Flag | Analysis, inherited by claims | Staleness reason | A successful rerun, or identical content upstream |
| `evidence moved` | Flag | Claim | One reason: newer version exists, upstream changed, or disk differs from the pinned version | `tm claim accept`, a rerun, or restoring the bytes |
| `cited file retired` | Flag | Claim | The cited identity is retired | `tm file restore`, or supersede or retract the claim |
| `assertion fails` | Flag | Claim | The assertion check failed on the cited or the new version | A revision, a supersede, or a retract |
| `text drifted` | Flag | Claim | Span text differs from the recorded sentence | `tm claim update-text`, or editing the span back |
| `points at a retracted claim`, `points at a superseded claim` | Flag | Span | Span ref is retracted or superseded | Edit the span |
| `promoted, not reviewed` | Flag | Claim | No review yet | Any review |
| `promoted with unapproved code` | Label | Claim | `--allow-unapproved` used | Permanent |
| `evidence changed since drafting` | Flag | Draft claim | Cited version's disk content changed between proposal and promotion | None while the draft is open: promotion is refused with `E6505`. The owner sends the item back or the session revises it to cite the new version |
| `unisolated` | Label | Session and its work | No sandbox | Permanent |
| `isolation unknown` | Label | Session and its work | Ended without a launcher confirmation; treated as `unisolated` | Permanent |
| `possibly agent-issued` | Label | Owner action or receipt | Harness variable set, or project-inbox receipt during a session | Permanent |
| `during unisolated session` | Label | Owner action | Issued while an unisolated session was open | Permanent |
| `pending viewer action` | Flag | Viewer journal event | Not yet read into the main chain | `tm apply --viewer`, or `tm status` after an interactive confirmation (3.2) |
| `rejected` | Label | Draft item | Sent back, with the note | Permanent; a later revision is a new event |
| `retracted` | Label | Claim | Owner retracted it, with the reason | Permanent |
| `superseded` | Label | Claim | A new claim points back at it | Permanent |

### 6. File attributes

Attributes split into identity level (about the file as a thing with a path or a declared output name) and version level (about one fingerprint). "Cited" and "current" are computed by the Core from claims and runs and are never stored on the file.

#### 6.1 Identity level

| Attribute | Values | Set by |
| --- | --- | --- |
| Kind | `input` (project path) or `output` (analysis, declared pattern, relative path) | Registration or declaration |
| Level | `tracked` (declared input or output, no title) or `registered` (title and description; citable) | Tracking is automatic; registration is an owner action (agents propose titles) |
| Retired | yes or no | Computed from the latest `retired` or `restored` label; `retired` is automatic when the analysis stops declaring the output, `restored` is `tm file restore` |
| Removable | yes or no | Owner |
| Agent-denied | yes or no | Owner setting |
| Parse spec | present or absent (tables only) | `tm table spec` |
| Panel names | list, on each image version | `tm file panels` |

#### 6.2 Version level

| Attribute | Values | Set by |
| --- | --- | --- |
| Fingerprint | `sha256:...`, or a directory manifest fingerprint | `tm file register` or `tm file hash` |
| Kept | yes or no, with `fingerprint-only` as an explicit owner choice | First promotion citing it, or `tm file keep` |
| Origin | `run R<n>`, `registered`, or `changed outside a run` | Files, from receipts or disk scans |
| Content label | `content changed`, `carried forward`, or `metadata only` | Files, by comparing with the previous version |

#### 6.3 Allowed combinations

| Level | Retired | Has a version | Kept | Citable by new claims | Visible to agents | Bytes guaranteed available |
| --- | --- | --- | --- | --- | --- | --- |
| tracked | no | any | no | No (needs registration) | Yes, as lineage only | No |
| registered | no | no (`--defer-hash`) | no | No | Yes, listed as unhashed | No |
| registered | no | yes | no | Yes | Yes | Only while on disk |
| registered | no | yes | yes | Yes | Yes | Yes, from the record |
| registered | no | yes | fingerprint-only | Yes, labeled `evidence not kept` | Yes | Only while on disk |
| registered | yes | yes | any | No | Yes, labeled `retired` | As above |
| any, removable | any | any | any | As above | As above | Only if kept; a deleted removable file is `expected-missing` |
| any, agent-denied | any | any | any | Yes for the owner | No; the tool server refuses it and the launcher does not mount it | As above |

A file cannot be both `tracked` only and kept (keeping requires registration first). A retired file cannot be registered again; it is restored, but a receipt from a run started before the retirement may still add a version to it (labeled `declaration changed during run`). Renaming an input is an explicit command and keeps versions.

### 7. Claim states and the evidence-exists outcome table

#### 7.1 States of a claim

```text
proposed (journal) --promote--> promoted --review--> reviewed
      |                            |
      +--send back--> rejected     +--retract--> retracted
      |                            +--supersede--> superseded (new claim added)
      +--revise--> proposed (new revision, same ref)
owner claim: added (main chain) == promoted
```

The current revision of a ref is the latest promoted revision, or, for a ref with no promoted revision, the latest proposal; the promote event names the exact revision event it accepts (D29). Any session may propose a revision of any unpromoted or sent-back ref. Revisions of one ref across journals are ordered by read-in order, then journal `seq`; a proposal response is provisional until read-in. A revision read in after the ref has a promoted revision is recorded as rejected with the reason "ref already promoted". Every revision event records its authoring session, which the views show beside the ref. Promoted claims change only by owner action.

#### 7.2 Checks per kind

| Kind | Evidence-exists check | Assertion check | Review |
| --- | --- | --- | --- |
| Quantitative | Yes | Yes | Yes |
| Qualitative | Yes | None (shown as "no system check") | Yes, the only line of judgment |
| Mixed | Yes | Yes, for the quantitative part | Yes |

#### 7.3 Evidence-exists outcome table

Rows are citation types; columns are the cited version's situation at check time. The check runs against the pinned version. "Disk differs" means the on-disk file no longer matches the pinned fingerprint.

| Citation type | Kept, or on disk and matches | On disk, disk differs | Missing, not kept | Removable and deleted, not kept | Fingerprint only (owner chose) and missing | Retired |
| --- | --- | --- | --- | --- | --- | --- |
| Cells by key | Holds if every key is present; fails if a key is absent | Holds or fails against the pinned bytes if kept; otherwise `not checkable` plus flag `evidence moved: disk differs` | `not checkable` | `not checkable`, labeled `expected-missing` | `not checkable` | As the matching column, plus flag `cited file retired` |
| Whole file | Holds | Holds against the pinned version; flag `evidence moved` | `not checkable` | `not checkable`, `expected-missing` | `not checkable` | As the matching column, plus flag `cited file retired` |
| Page | Holds if page <= page count of the pinned version; fails otherwise | As cells | `not checkable` | `not checkable` | `not checkable` | As the matching column, plus flag `cited file retired` |
| Panel | Holds if the label is registered on the pinned image version; fails otherwise | Holds (labels are metadata); flag `evidence moved` | `not checkable` | `not checkable`, `expected-missing` | `not checkable` | As the matching column, plus flag `cited file retired` |
| Reserved (quote, page region, image region) | `not checkable` | `not checkable` | `not checkable` | `not checkable` | `not checkable` | `not checkable` |

An absent key is `E6003`, a refusal, only when `tm claim add` first validates the citation, which makes it a typo in the command; every check after that, including a check against a version the key has since left, records the outcome in this table instead of refusing. A claim is never shown as "holds" from this table unless the cell says holds. `error` is reserved for failures of the checker itself (parse error, DuckDB error) and is shown with the error code.

### 8. Staleness rules

1. **Unit.** Staleness belongs to an analysis, judged against its latest successful run (exit status zero, run check passed or acknowledged).
2. **Own triggers.** The analysis is `own code changed` when any declared script or support script's current fingerprint differs from the one its latest run used; `command changed` when the command template differs (marked variable parts are ignored, the expanded command is recorded but not compared); `own inputs changed` when any declared input's current version differs from the one the run used, including config and parameter files, and including inputs `changed outside a run`.
3. **Upstream trigger.** The analysis is `upstream changed` when any declared input is an output of an analysis that is itself out of date, or whose latest successful run produced a different version from the one this analysis used.
4. **Cutoff.** The spread stops at identical content: a rerun whose output version is `carried forward` (same bytes, same table after sorting by key, or same decompressed content for `.gz` and `.bgz`) leaves downstream analyses unchanged.
5. **Lineage per version.** Edges connect output versions to the input versions a run used, so an analysis that updates a file in place never makes a loop.
6. **Environment.** The software environment is recorded on each run and compared by `tm run again`, which warns on a difference; it is not a trigger.
7. **Failed runs.** A failed or interrupted run does not make its outputs current and does not clear staleness.
8. **Where it is computed.** The Core computes staleness from the index plus the latest disk scan, on demand, for `tm status`, session start, the viewer, agent queries, and audit. Disk scans are cached with their age; `tm status --refresh` re-scans.
9. **Claims inherit.** A claim citing an output version of an out-of-date analysis inherits the flag with the same reason, and agent proposals on such versions carry it in the response.
10. **Reporting.** Out-of-date analyses are listed in dependency order with their recorded commands. `tm status` suggests narrowing a directory input whose changes are all in entries the run's outputs never depended on (heuristic: entries changed but no output version changed on the last rerun).

### 9. Core operations table

Each row is one Core function. "Permission" names what the Core checks before any library call. Library calls are in order. Events are the batch written at the end (main chain for owner operations, the caller's journal for sessions and viewer launches, receipts for the wrapper). Errors are catalog codes; the code scheme is in 11.12, the codes are listed per row below, and the generated `docs/errors.md` collects them. Owner operations resolve their targets by event ID and confirm that the ID exists on the chain or the named journal before writing; the index is used only to find candidates. Rows marked "internal" are HTTP endpoints of `tm view --serve`, not commands.

| Operation | Actor | Permission | Library calls, in order | Events | Errors |
| --- | --- | --- | --- | --- | --- |
| `project.init` | Owner | `OwnerCap`; record path writable, not inside the repo; `~/.config/toulmin/` writable | Record: create layout, header, HEAD; write `.toulmin/project.json` and the binding under `~/.config/toulmin/projects/` (with `--attach`: check `record.json`, write the binding only) | `project.created`, `settings.changed` | `E2001` record exists, `E2002` path inside repository, `E2003` filesystem unsuitable, `E2004` binding missing, `E2005` repository root not bound, `E2006` binding conflicts or config directory unwritable |
| `settings.set` | Owner | `OwnerCap` | None | `settings.changed` | `E7001` unknown key, `E7002` bad value |
| `read_in` (implicit in every locked command) | Owner | Lock held, epoch valid, `OwnerCap` from the calling command | Record: truncate a torn tail found at lock time; Record: list new journal lines and inbox; Files: version outputs from receipts; Runs: create runs, run checks; Claims: read in reviews; Core: turn viewer actions into events; each library's fold | `chain.torn_tail_truncated` when one was found, `journal.head_recorded` per journal, `receipt.ingested`, `run.started`, `run.finished`, `run.checked`, `file.version_added`, `claim.reviewed`, the viewer actions' events with `source: viewer` | `E2101` torn tail, `E2102` fork, `E4001` malformed receipt (quarantined, not fatal) |
| `file.register` | Owner | `OwnerCap`; path inside project; title and description present | fsfacts: stat and hash in the foreground (skipped with `--defer-hash`); Files: create identity, and the first version unless deferred; Files: checksum compare if given | `file.registered`, `file.version_added`, `file.checksum_checked` | `E3001` outside project, `E3002` missing title, `E3003` non-UTF-8 path, `E3004` checksum mismatch |
| `file.hash` | Owner | `OwnerCap`; file registered with no version yet, or `--again` | fsfacts: hash and manifest; Files: add the version; Files: checksum compare if given | `file.version_added`, `file.checksum_checked` | `E3005` already hashed |
| `table.spec` | Owner | `OwnerCap`; file registered and has a version | Files: parse with spec, check key uniqueness, write parse cache | `file.parse_spec_set` | `E3101` duplicate key (with values), `E3102` type parse failure (row, column), `E3103` unknown column, `E3104` unsupported encoding, `E3105` invalid parse spec |
| `file.keep`, `file.removable`, `file.restore`, `file.rename` | Owner | `OwnerCap`; valid state transition per section 6 | Record: copy and compress bytes (keep); Files: set attribute | `file.kept`, `file.marked_removable`, `file.restored`, `file.renamed` | `E3201` invalid transition, `E3202` size above budget, `E3203` bytes missing |
| `file.panels` | Owner | `OwnerCap`; image version | Files: set labels | `file.panels_set` | `E3301` not an image |
| `analysis.declare` | Owner | `OwnerCap`; paths inside project | fsfacts: expand globs, directories, list files; Files: track inputs and outputs; Runs: create analysis and code version | `analysis.declared`, `file.tracked` per new path | `E4101` path outside project, `E4102` output overlaps another analysis |
| `run.exec` (the wrapper) | Owner, any node | No lock, no `OwnerCap` needed to write a receipt; declaration must exist | fsfacts: fingerprint scripts, snapshot deps and outs; Runs: git facts (commit, diff); write started receipt; exec; fsfacts: snapshot, hash changed; write finished receipt | Receipts only | `E4201` analysis unknown, `E4202` git unavailable (recorded, not fatal), `E4203` inbox unwritable (falls back to project inbox) |
| `run.finish` | Owner | `OwnerCap`; partial receipt exists | Core: if the recorded hashing estimate exceeds the login-node limit (setting), print a resubmit line and stop; else fsfacts: hash remaining outputs; write finished receipt | Receipt, then `run.finished` at read-in | `E4204` no partial receipt, `E4205` resubmit printed |
| `run.ingest` | Owner | `OwnerCap` | As `read_in` for the given path | As `read_in` | As `read_in` |
| `run.approve` | Owner | `OwnerCap`; code version exists | Runs: resolve code version; Runs: attribution for the diff view; Core: staleness of the analysis | `run.approved` | `E4301` unknown code version, `W4302` analysis out of date (warning) |
| `run.review` | Owner, viewer | `OwnerCap` or viewer confirmation | Runs: validate | `run.reviewed` | `E4401` unknown run |
| `run.again` | Owner | `OwnerCap`; run exists | Runs: recorded command and environment; print resubmit line if batch; else `run.exec` | As `run.exec` | `E4501` environment differs (warning), `E4502` recorded in batch (prints resubmit, exits) |
| `session.start` | Owner | `OwnerCap`; lock | Core: allocate `S<n>` and token; Record: create journal; emit handshake start | `session.started` | None |
| `session.serve` | Owner (launcher calls `tm`) | Session open and confirmed; no `OwnerCap` (the command execs `tm-agent`, which holds none) | Core: check the session is open and confirmed; exec `tm-agent` in the foreground with the session's socket directory and token | None | `E7105` session not open or not confirmed |
| `session.confirm` | Owner (launcher calls `tm`) | `OwnerCap`; session open and unconfirmed | Core: record preset, isolation, network mode, container paths of the socket directory and bridge, and the hash of the planned mount list | `session.confirmed` | `E7101` lists not applied, `E7103` already confirmed |
| `session.end`, `session.close`, `session.abandon` | Owner, or launcher via `tm` (see below) | `OwnerCap` | Runs: compute patch IDs from the shared `.git` between base and head, match commits | `session.ended` or `session.closed` or `session.abandoned`, `session.commits_recorded` | `E7102` repository unreadable |
| `viewer.launch`, `viewer.end` (internal) | Owner | `OwnerCap`; lock | Core: allocate `V<n>` and token (written to `/tmp/toulmin-$UID/V<n>/token`, mode `0600`); Record: create journal | `viewer.started`, `viewer.ended`; `viewer.abandoned` at read-in when a launch has no end event after the session timeout (OQ3) | None |
| `claim.add` | Owner | `OwnerCap`; cited versions citable; parse specs present for cells | Claims: validate; Files: tables as Arrow, key presence, page counts, panel labels; Checker: run; Claims: record claim and verdict; Claims: scan report for the ref | `claim.added`, `claim.checked` | `E6001` version not citable, `E6002` missing required choice (field), `E6003` a `cells` citation names a key absent from the pinned version at add time, `E6004` type mismatch, `E6005` unknown column, `E6006` expression syntax error, `E6007` malformed citation, `E5001` checker error |
| `claim.propose`, `claim.revise` | Session | Session token; cited handles are registered, not agent-denied; limits | Claims: validate; Files; Checker; Claims: verdict and flags | Journal: `claim.proposed` or `claim.revised` | `E6101` agent-denied, `E6102` host path given, `E6103` limit exceeded, plus `claim.add` errors |
| `title.propose`, `declaration.draft`, `supersede.propose`, `accept.propose`, `note.draft` | Session | Session token; paths inside project | Claims or Runs: validate | Journal events | `E4101`, `E6101` |
| `claim.check` (re-check) | Owner, or Core after a new version | `OwnerCap` from the calling command (system-initiated) | Files; Checker; Claims | `claim.checked` | `E5001` checker error, `E5002` arithmetic overflow |
| `claim.review` | Owner, viewer | `OwnerCap` or viewer confirmation | Claims | `claim.reviewed` | `E6201` unknown ref |
| `claim.update_text` | Owner | `OwnerCap`; promoted claim | Claims: scan span, record sentence; Files; Checker; Claims | `claim.revised`, `claim.checked` | `E6201`, `E6301` span not found |
| `claim.retract` | Owner | `OwnerCap` | Claims | `claim.retracted` | `E6201` |
| `claim.supersede` | Owner | `OwnerCap` | As `claim.add`, then Claims: link | `claim.added`, `claim.checked`, `claim.superseded` | As `claim.add` |
| `claim.accept` | Owner | `OwnerCap`; new version exists and is citable | Files; Checker (re-check on the new version); Claims | `claim.accepted`, `claim.checked` | `E6401` no newer version |
| `draft.promote` | Owner | `OwnerCap`; selection closed under dependencies; code approved or `--allow-unapproved` | Claims: resolve journal events and current revisions; Core: dependency closure and approval gate; Files: confirm each cited version's bytes still match its pinned fingerprint; Checker: re-check against pinned versions; Files: keep cited versions on first promotion (Record copies); Claims: record | `draft.promoted` (with override field), `file.kept`, `claim.checked`, `claim.added` per promoted claim | `E6501` dependency missing, `E6502` unapproved code, `E6503` already promoted, `E6504` revision not current, `E6505` evidence changed since drafting |
| `draft.send_back` | Owner | `OwnerCap` | Claims | `draft.sent_back` | `E6201` |
| `report.add` | Owner | `OwnerCap`; file in repo | Claims: scan spans | `report.registered` | `E6601` not a Markdown file |
| `report.check` | Owner | None (reads) | Claims: scan spans, drift per span | None (`tm status` raises the flags) | `E6201` |
| `analysis.edit` | Owner | `OwnerCap`; analysis exists | As `analysis.declare`; Runs: new code version | `analysis.declared` (new code version) | As `analysis.declare` |
| `run.from_draft` | Owner | `OwnerCap`; drafted declaration in a journal | `analysis.declare` from the draft, then `run.exec` | As both | As both |
| `doctor`, `lock.status`, `settings.show`, `settings.unset` | Owner | None (reads), except `settings.unset` (`OwnerCap`) | fsfacts and Record probes; Record: read lock; settings | `settings.changed` for unset | None |
| `view.export` | Owner | `OwnerCap` (reads only) | Read interface | None | `E8001` output dir not empty |
| `apply` | Owner | `OwnerCap`; action file schema | Each action's operation, after confirmation | Each action's events with `source: static-view` | `E8101` outdated action (target changed since export) |
| `viewer.confirm_action` (internal) | Owner via viewer | Per-launch token, one-time code (printed to the terminal running `tm view --serve`, never sent to the browser), Host, and Origin | Core: validate the action now; write to journal `V<n>` | Journal: `viewer.action_confirmed` | `E8201` bad code, `E8202` origin mismatch |
| `audit` | Owner | `OwnerCap` (reads, plus one event) | Record: verify chains, journals, kept files; fsfacts: full re-hash; Files: compare; Checker: re-run every claim check; Claims: compare verdicts; Core: staleness | `audit.completed` with report hash | `E2101`, `E2102`, `E2103` corruption, `E3401` hash mismatch (reported) |
| `engine.upgrade` | Owner | `OwnerCap` | Checker: re-run every check with the new engine; Claims: diff | `engine.upgraded` with changed-verdict report | `E5101` engine not pinned |
| `lock.break` | Owner | `OwnerCap`; heartbeat unchanged past the takeover timeout | Record: increment epoch | `lock.taken_over` | `E2201` heartbeat still changing |
| `status` | Owner | Lock (for read-in) | `read_in`; fsfacts: disk scan; Core: staleness, flags | As `read_in` | None |
| Read interface queries | Owner, session, viewer | Session: agent-denied filter and limits | Index plus open journals; Files for bytes and tables; Checker for `preview_steps` and for `claims.evidence` (per-step rows are recomputed from the parse cache) | None | `E6101`, `E6103` |

`session.end` is called by the launcher with the end message; `session.close` is the owner's command for a crashed launcher, giving base and head by hand; `session.abandon` is automatic at read-in after the OQ3 timeout.

### 10. Read-interface query list

One Core function per row. The CLI, the MCP tool schemas, and the view pages are derived from this list; a test checks that all three return the same data for the reference project. "Agent" marks queries the tool server exposes, always filtered by the agent-denied list and the session limits.

| Query | Parameters | Returns | Agent |
| --- | --- | --- | --- |
| `files.list` | level, kind, cited, retired, analysis, text search | File identities with title, description, source, latest version, attributes, cited count | Yes |
| `files.show` | file handle or path | Identity, all versions with fingerprints, attributes, labels, flags, parse spec, panel labels | Yes |
| `files.lineage` | version handle, depth | Tree from the version back to registered inputs: runs, code versions, approval state, labels per edge | Yes |
| `files.rows` | version handle, key values or row range, columns | Rows from the parse cache, with neighbors | Yes (limits) |
| `files.bytes` | version handle | Kept or on-disk bytes | Owner only |
| `analyses.list` | staleness, approval state | Analyses with latest run, code version, staleness reason | Yes |
| `analyses.show` | analysis handle | Declaration, code versions, runs, approval history | Yes |
| `runs.list` | analysis, state, since | Runs with status, labels, flags, launch context | Yes |
| `runs.show` | run handle | Receipts, snapshots, hashes, run check, diff of declared scripts with agent lines marked, environment | Yes |
| `runs.diff_since_approved` | code version | Diff and attribution | Yes |
| `staleness.list` | none | Out-of-date analyses in dependency order with recorded commands and reasons | Yes |
| `claims.list` | ref prefix, state, kind, cited version, flag, report, session | Claims with current revision, verdicts, flags, reviews | Yes |
| `claims.show` | ref | Full claim, all revisions, verdicts with SQL and versions, reviews, flags with reason and next command, spans | Yes |
| `claims.evidence` | ref, limit (default `view.rows_per_page`) | Cited rows in full rows with neighbors, capped at `limit`, with `truncated` giving the number of cited rows not returned; per-step outputs and counts, descriptions, assertion result | Yes |
| `claims.citing_rows` | version handle, key values | Other claims citing the same rows | Yes |
| `claims.history` | ref | Events in order | Yes |
| `reports.list` | none | Registered reports with span counts and drift counts | Yes |
| `reports.spans` | report | Spans with refs, recorded sentence, current text, drift state, in file order: by line, then by position on the line | Yes |
| `sessions.list` | state | Sessions with isolation, harness, counts, open rejections | Yes (own and others' summaries) |
| `sessions.rejections` | session or all | Sent-back items with notes | Yes |
| `flags.list` | kind, subject | Open flags with reasons and next command | Yes |
| `settings.show` | none | Settings with the event that set each | Yes (read) |
| `audit.last` | none | Summary of the last audit: `counts`, `since`, `report_hash`, and the report's path relative to an export directory (`audit/<report_hash>.html`) | Yes |
| `pending.list` | none | Pending viewer actions, receipts in `new`, quarantined receipts | Owner only |
| `checker.preview_steps` | tables (version handles), steps | Per-step outputs and counts without recording anything | Yes (limits) |
| `checker.describe_steps` | steps | Plain-language description and validation result | Yes |

A flag is returned in one shape wherever it appears, the shape `flags.list` returns: kind, subject, reason, and next command. One renderer serves every page, and the G9 parity test covers the shape once.

MCP tools in `tm-agent`: `query` (dispatches to the agent-allowed rows above by name), `preview_steps`, `propose_claim`, `revise_claim`, `propose_title`, `propose_accept`, `propose_supersede`, `draft_declaration`, and `draft_note`. Each tool schema carries the typed value, citation, step, and assertion schemas from `tm-formats`, so the schema is the agent's guide to writing valid claims.

### 11. Shared-formats decision sheet

These are the choices `tm-formats` encodes. They are needed before any other code. Mechanical choices with one defensible answer are made here; none reopens a numbered decision. Schemas written as `{"a","b"}` list keys only; fenced blocks are examples with values.

#### 11.1 Canonical JSON

Canonical JSON is RFC 8785 (JCS), implemented in-house and tested against the RFC's examples and a second Python implementation. Hashed records hold no JSON floats: integers within plus or minus 2^53 are JSON integers; everything else is a string. Times are RFC 3339 strings in UTC with nanoseconds as a separate integer field where sub-second precision is recorded (snapshots and receipts). Non-UTF-8 paths are refused with `E3003`. Input to the canonicalizer must be I-JSON (RFC 7493): valid UTF-8, no duplicate member names, no lone surrogates, and no `NaN` or `Infinity`; anything else, including any JSON syntax error, is `E1003`. A number token with a fraction or an exponent, a minus zero (`-0`), or an integer outside plus or minus 2^53 (inclusive) counts as a float for this rule and is `E1001`. When an input has several faults, the one reported is the first a left-to-right read meets, except that a duplicate member name is reported when its object closes and a lone surrogate only after the whole input is read.

#### 11.2 Event envelope

```json
{
  "env": 1,
  "type": "claim.proposed",
  "tv": 1,
  "chain": "S4",
  "seq": 128,
  "prev": "sha256:...",
  "at": "2026-11-09T14:02:11Z",
  "actor": {"kind": "session", "id": "S4"},
  "src": {"source": "cli", "tty": true, "harness_env": false},
  "body": { }
}
```

- `env` is the envelope version. `type` is namespaced by owning library (`file.`, `run.`, `claim.`, `draft.`, `session.`, `viewer.`, `journal.`, `chain.`, `settings.`, `project.`, `engine.`, `audit.`, `lock.`). `tv` is the event type's own version.
- `chain` is `main`, `S<n>`, or `V<n>`. `seq` starts at 1 per chain. `prev` is the previous event's ID on the same chain; the first event uses the chain's genesis string, `"sha256:" + hex(sha256(canonical({"chain":<chain>,"project_id":<project ID>})))`.
- `actor.kind` is `owner`, `session`, or `viewer`. `src` carries the D23 facts for owner actions and `source` in {`cli`, `static-view`, `viewer`, `wrapper`}.
- **ID.** `id = "sha256:" + hex(sha256(canonical(envelope without id)))`. The line is the canonical envelope with `id` appended as the final key after serialisation, so the line itself is not JCS-canonical; readers strip `id`, recompute, and compare, and a mismatch is corruption.
- Events in the main chain also carry `batch` (the batch number) and `epoch`.
- **Validation** (`E1002`, with `field` the JSON Pointer of the offending member, or of where an absent member belongs). The keys are exactly those of the example, plus `batch` and `epoch` on the main chain and only there, plus the reserved `sig` and `collab` (11.13) when present. `env`, `tv`, `seq`, `batch`, and `epoch` are JSON integers, `env`, `tv`, and `seq` at least 1, `batch` and `epoch` at least 0; `type` matches `[a-z]+\.[a-z][a-z_]*`; `chain` matches `main`, `S[1-9][0-9]*`, or `V[1-9][0-9]*`; `prev` and `id` match `sha256:[0-9a-f]{64}`; `at` is `YYYY-MM-DDTHH:MM:SSZ` (UTC, whole seconds); `actor` is exactly `{"kind","id"}` with a non-empty string `id`; `src` is exactly `{"source","tty","harness_env"}` with boolean `tty` and `harness_env`; `body` is an object. A line must be byte-identical to the canonical envelope without `id` with `,"id":"<id>"` inserted before its final `}`; any other form, or an `id` that does not match, is `E1002` at `/id` in `tm-formats` (the Record library reports a mismatch on a stored line as `E2103` corruption). An envelope whose `env` is above 1 is opaque and only its line form and `id` are checked; one whose `type` is unknown, or whose `tv` is above the highest known for its type, is opaque (11.11) and its envelope is checked but its body is not; any other unknown `tv` is `E1002` at `/tv`.

#### 11.3 Chain link and batch framing (main chain)

- Segment files `main/seg-NNNNNN.jsonl`, sealed before a batch that would carry them past 64 MiB (a single larger batch is its own segment); sealed segments are packed with zstd to `seg-NNNNNN.jsonl.zst` and the original removed after the packed file is synced and verified.
- A batch is: one header line `{"b":<batch>,"n":<events>,"len":<payload bytes>,"h":"sha256:...","epoch":<e>}`, then exactly `len` bytes of newline-terminated events. `h` is the hash of the payload. A header whose `len` is not satisfied, or whose `h` does not match, is a torn tail.
- `main/HEAD` is a small JSON file rewritten by write-then-rename after every batch: `{"seq","segment","offset","last_id","batch","epoch","format":1}`.
- `main/lock` holds `{"host","pid","started","heartbeat","epoch"}`.
- Record header `record.json`: `{"format":1,"project_id","owner_id","created","hash_alg":"sha256"}`.

#### 11.4 Journal format

`journals/S4.jsonl` and `journals/V2.jsonl`: one envelope per line, no batch framing, `prev` links lines. The main chain's `journal.head_recorded` body is `{"journal":"S4","seq":128,"id":"sha256:..."}`. Read-in accepts only the event types allowed for the journal's kind (session journals: `claim.proposed`, `claim.revised`, `title.proposed`, `declaration.drafted`, `supersede.proposed`, `accept.proposed`, `note.drafted`, `journal.torn_tail_truncated`; viewer journals: `claim.reviewed`, `run.reviewed`, `viewer.action_confirmed`); any other type is recorded on the main chain as `journal.event_quarantined` with the offending line's ID and is never interpreted.

#### 11.5 Receipt format

`inbox/new/<sha256>.json`:

```jsonc
{
  "receipt": 1,
  "phase": "started" | "finished" | "partial",
  "attempt": "<128-bit random hex>",
  "analysis": "A3",
  "declaration": "sha256:<id of the analysis.declared event>",
  "part": null,
  "code": {"scripts": {"scripts/de.R": "sha256:..."}, "template": "...", "expanded": "...",
           "deps": ["..."], "outs": ["..."], "excludes": ["..."]},
  "git": {"commit": "...", "dirty": true, "diff": "sha256:<kept blob>"},
  "context": {"kind": "batch", "scheduler": "slurm", "script": "run_de.sh", "env": {"SLURM_JOB_ID": "..."}},
  "node": {"host": "...", "pid": 12345, "started": "..."},
  "before": {"deps": {"path": {"size":"...","mtime":"...","ctime":"...","ino":"..."}}, "outs": {}},
  "after": {"deps": {}, "outs": {}},
  "hashes": {"path": "sha256:..."},
  "manifests": {"results/de/": "sha256:..."},
  "exit": 0,
  "hashing_seconds": "12.4",
  "env_summary": {"PATH_hash": "...", "modules": [], "conda": null}
}
```

`started` carries `before` and no `after`; `finished` carries both; `partial` carries `after` snapshots without hashes. The `attempt` links phases. `code.deps`, `code.outs`, and `code.excludes` are the frozen expanded lists the wrapper ran against. Read-in checks the run against the declaration named in the receipt, not the current one; when they differ, the run is labeled `declaration changed during run`, outputs the current declaration no longer declares are versioned under their (retired) identity with that label, and the run check reports the difference. The script diff is stored as a kept blob by the wrapper into `inbox/blobs/` and moved by read-in into `objects/sha256/`.

#### 11.6 Typed values

One encoding everywhere (events, steps, assertions, Arrow conversion):

| Type | Encoding | Notes |
| --- | --- | --- |
| `int` | `{"t":"int","v":"42"}` | Decimal string; arbitrary size |
| `dec` | `{"t":"dec","v":"0.00310"}` | Decimal string, always positional notation, never an exponent. Parsed values keep the scale as written, trailing zeros included. Computed values follow the General Decimal Arithmetic specification (IEEE 754-2008 decimal) at 34 significant digits, rounding half-even, so multiplication's scale is the sum of the operands' scales (`1.10 * 2` is `2.20`), addition and subtraction take the larger scale, and division is exact when it terminates within 34 digits and otherwise rounds (`10 / 4` is `2.5`). Negative zero is written without its sign, scale kept (`-0.00` is `0.00`). A value that reaches this encoding from an IEEE double (`log`, `exp`, `mean`, `median`) is first rounded to 15 significant digits, then written with the shortest digits that round-trip at that precision, in positional notation |
| `str` | `{"t":"str","v":"..."}` | NFC-normalized |
| `bool` | `{"t":"bool","v":true}` | |
| `list` | `{"t":"list","v":[typed values]}` | Settings only in gate A; never a column value |
| `date`, `datetime` | `{"t":"date","v":"2026-11-09"}` | RFC 3339 |
| `missing` | `{"t":"missing"}` | Null, NA tokens, and anything the parse spec maps to missing |
| `nan`, `+inf`, `-inf` | `{"t":"nan"}`, `{"t":"inf","s":"+"}` | Only when the parse spec maps them to a value instead of missing; treated as missing by filters and placed by the sort's missing rule |

Lexical forms, checked wherever a typed value is read (`E1004` with `field` at the offending member; `E1002` inside an envelope line): an object has exactly the keys shown for its `t`. `int` `v` matches `-?(0|[1-9][0-9]*)` and `dec` `v` matches `-?(0|[1-9][0-9]*)(\.[0-9]+)?`; in both, a minus sign before digits that are all zero is refused. `str` `v` must already be NFC. `date` `v` is `YYYY-MM-DD`, a valid proleptic Gregorian date in years 0001 to 9999; `datetime` `v` is `YYYY-MM-DDTHH:MM:SS`, then optionally `.` and one to nine fraction digits with no trailing zero, then `Z` (UTC only; a whole second carries no fraction). `list` elements are typed values other than `list`. `s` of `inf` is `+` or `-`.

Floats from a table are parsed as decimals from their text; a parse spec may declare a column `float64`, in which case the value is the shortest round-trip decimal string of the IEEE value, in positional notation, with either zero written `0`. Arithmetic in `derive` is decimal with a stated precision (34 digits), except `log` and `exp`, which are IEEE double; `mean` and `median` in `group` are also IEEE double in DuckDB. These four are the only places last-digit differences are tolerated. Decimal columns map to DuckDB `DECIMAL(38, s)` with `s` fixed per column as the largest scale seen when the parse spec is set; a value that does not fit at that scale is a parse failure (`E3102`), and an arithmetic overflow in `derive` gives verdict `error` with `E5002`. The differential suite runs the same cases on two CPU generations, since DuckDB selects code paths by CPU feature at runtime.

#### 11.7 Parse spec

`{"spec":1,"format":"csv|tsv|parquet","delimiter","header":true,"columns":[{"name","type","missing":["NA",""]}],"key":["gene_id"],"nan":"missing|value","inf":"missing|value","encoding":"utf-8"}`. `type` is one of `int`, `dec`, `str`, `bool`, `date`, `datetime`, or `float64` (item 47's mapping); `missing` is the list of tokens read as `missing` in that column; a `dec` column also carries `scale`, a JSON integer from 0 to 38 that `table.spec` records as the largest scale seen (item 47), and no other type carries it. Every key shown is required: `delimiter` is a one-character string and `header` a boolean for `csv` and `tsv`, and both are `null` for `parquet`. Column names are non-empty and distinct; `key` is a non-empty list of distinct column names. A key name the columns list does not have, or a column the file's header does not have, is `E3103`; any other schema violation is `E3105`; `field` points at the offending member. `utf-8` is the only encoding accepted in V1; another value is refused with `E3104`. Its fingerprint is the ID of its canonical JSON. Key uniqueness is checked at `table.spec` and again when a new version arrives.

#### 11.8 Query steps

`{"steps":1,"tables":{"de":"F7@v2"},"from":"de","ops":[...]}`. `from` names the table the pipeline starts from and is required even when `tables` holds one entry; a `from` that names no entry in `tables` is `E6002`. A claim may carry an assertion with `steps` null: the final table is then the rows its `cells` citation names, in key order, with no pipeline and no SQL, which is how a single-cell `value` assertion is written. Operations and their versions:

| Op (version) | Required fields |
| --- | --- |
| `filter` (1) | `where`: expression; missing never matches |
| `select` (1) | `columns` |
| `sort` (1) | `by`: list of `{column, dir}` with `dir` one of `asc` or `desc`; `missing`: `first` or `last`; the table key is appended as the last tie-breaker automatically and recorded in the SQL |
| `take` (1) | `n`; requires a preceding `sort` or `rank` (total order) |
| `rank` (1) | `by`, `dir`, `ties`: `min`, `max`, `dense`, or `ordinal` (ordinal uses the key as tie-breaker), `missing`: `first`, `last`, or `exclude`, `scope`: `all` or `group`, `group_by`: column list, required when `scope` is `group` |
| `derive` (1) | `as`, `expr` from a closed grammar: arithmetic, comparison, `abs`, `log`, `log2`, `log10`, `exp`, `coalesce`, string equality; no `random()`, `now()`, or user functions |
| `group` (1) | `by`, `aggregates`: list of `{as, fn in count,sum,mean,min,max,median, column}`; output ordered by group keys; each row carries member count and member-key fingerprint |
| `join` (1) | `right`: table name, `on`: `[{"left":"col","right":"col"}, ...]`, `kind`: `inner` or `left`; both sides must be keyed and the pairs must cover the right key |

Each op is an object `{"op":<name>,"v":<version>, ...}` holding exactly `op`, `v`, and its required fields (and `group_by` only when `scope` is `group`). `filter.where` and `derive.expr` are expression strings (11.18.1); `select.columns` and `rank.group_by` are non-empty lists of column names; `rank.by` is one column name; `sort.by` is a non-empty list of `{"column","dir"}` objects; `take.n` is a JSON integer of at least 0; `group.by` is a non-empty list of column names and `group.aggregates` a non-empty list; `join.on` is a non-empty list. `tables` is a non-empty object from table name to version handle, and `ops` is a list, possibly empty.

Validation codes, each with `field` the JSON Pointer (11.12) of the offending member, or of where an absent member belongs: `E6002` for a required member that is absent, a required list that is empty, a choice not among its listed values, an unknown `op` or op version, a key the op does not define, a `take` with no earlier `sort` or `rank`, a `join` whose `right` names no entry of `tables` or whose `on` pairs do not cover the right table's key, and a step that would add a column whose name is already present; `E6004` for a member of the wrong JSON type or form (a string that is not a version handle, a negative number where a non-negative one is required) and for an operand type error under 11.18.2; `E6005` for a column name the table does not have at that step; `E6006` for an expression that does not parse under 11.18.1. Checks that need no parse spec are made wherever steps are read; `E6005`, the operand types, and the join cover are checked against the cited tables' parse specs before anything runs.

Each op's semantics are frozen at its version; a change is a new version number, and the old one stays runnable.

#### 11.9 Assertion schema

`{"assert":1,"kind":...}`:

| Kind | Fields | Meaning |
| --- | --- | --- |
| `value` | `row` (key values), `column`, `op`, `expected`, `tol` | The cell compares to `expected` |
| `rank` | `row`, `expected`, `tol` | The row's rank in the final table equals `expected` |
| `count` | `op`, `expected` | Final row count compares |
| `compare_rows` | `left` row and column, `right` row and column, `op`, `tol` | Two cells compare |
| `exists` | `row` | The row is present in the final table |

`op` in {`eq`, `ne`, `lt`, `le`, `gt`, `ge`, `between`}. Numeric comparison rule: both sides are decimals; `tol` is `{"abs":"0","rel":"0"}` by default and must be stated to be non-zero; `eq` with tolerance means `|a - b| <= abs + rel * |b|`; a missing value never satisfies any comparison and yields `fails`, not `not checkable`.

Shapes: `row` is a list of typed values, one per key column of the final table in its key order (11.18.2 gives the output key of each op); `column` is a column name; `left` and `right` are `{"row","column"}` objects; `expected` is a typed value, an `int` for `rank` and `count`, and for `between` a two-element JSON list `[lo, hi]` of typed values; `expected` is never `missing`, `nan`, `inf`, or `list`. `tol` is optional and, when present, exactly `{"abs","rel"}`, each a non-negative decimal in the 11.6 `dec` `v` form; `count` and `exists` take no `tol`. `rank` takes no `op` and compares as `eq`; `compare_rows` does not take `between`. The keys are exactly those listed per kind plus `assert` and `kind`. Validation codes follow 11.8: `E6002` for an absent, unknown, or not-allowed member or an empty `row`, `E6004` for a wrong JSON type or form or a value whose type the comparison does not allow. An operator that the type of `expected` does not allow is `E6004` at `/op`, and a `tol` it does not allow is `E6004` at `/tol`. At evaluation (11.18.3), a cell whose type does not compare with `expected` is `E6004` at `/expected`, or at `/right` for `compare_rows`, and a `tol` on a cell that is not a number is `E6004` at `/tol`.

#### 11.10 Text sort order

Binary order of NFC-normalized UTF-8 bytes, which is DuckDB's default binary collation. No locale collation anywhere. The SQL sets `ORDER BY ... COLLATE BINARY` explicitly.

#### 11.11 Version policy

Every format carries its own version number: record header, envelope, each event type, receipts, parse spec, steps, each op, assertions, handshake, action file. Readers accept any version they know; an unknown event type or a higher version is read as opaque and never rewritten, and writing an event with an unknown version is refused. Index folds skip opaque events and record that they did.

#### 11.12 Error catalog

Stable codes `E<library><nnn>`: 1 formats, 2 record, 3 files, 4 runs, 5 checker, 6 claims, 7 core and permissions, 8 front ends. Each error is `{"code","message","rule","field","owner_command","subject"}`; `rule` and `owner_command` are filled for agent refusals. `field` is a JSON Pointer (RFC 6901) into the document the error is about, for example `/ops/1/ties`, and is the same string in both the Rust and the reference implementation. Each catalog entry also carries a `gate` of `A`, `B`, `C`, or `D`, the earliest gate at which that code can be produced, so a gate's conformance test can filter the catalog to the codes it is allowed to expect. `field` is `null` when the input is not a JSON document (a CLI argument) and for `E1001` and `E1003`, which are about the input as JSON text. The formats codes are `E1001` float or out-of-range number in canonical JSON, `E1002` invalid envelope, `E1003` input not I-JSON, and `E1004` invalid typed value. An error inside an envelope line is reported as `E1002` with the pointer into the line, whatever code the nested object's own validation would give standing alone. Messages may change; codes never do.

#### 11.13 Reserved fields

Envelope: `sig`, `collab`. Run body: `parent_run`. Citation kinds: `quote`, `page_region`, `image_region`. Verdict body: `model_check`. Session body: `isolation: separate-host`. Reserved fields are accepted and stored as given, never interpreted in V1.

#### 11.14 Handshake schema

Start (Toulmin to launcher, printed as JSON by `tm session start`):

```json
{"handshake":1,"session":"S4","token":"...","bridge":"/path/to/tm-bridge",
 "socket":"/tmp/toulmin-1000/S4/sock",
 "mount_socket_dir":"/tmp/toulmin-1000/S4",
 "agent_denied":["data/patients.tsv"],
 "must_not_mount":["/shared/records/proj","~/.cache/toulmin","~/.config/toulmin","/tmp/toulmin-1000/*"],
 "project_root":"/home/tim/proj","role":"writer"}
```

Every path in the start message is an absolute path resolved for this host (the record directory, the index and stat-cache directory in use, the config directory, and `/tmp/toulmin-$UID/*`); the example uses `~` for brevity only. `mount_socket_dir` is the one exception to the `/tmp/toulmin-1000/*` entry: the launcher mounts that directory (at a container path of its own choosing, reported back in `container_paths`), never the socket file, so a restarted tool server's new socket is visible inside the sandbox. The `bridge` path is a host path; the launcher mounts the binary read-only and reports its container path the same way. `network` is one of `locked` (model API only), `packages`, or `open`; agentbox has no network-off mode. `mounts_hash` is the hash of the launcher's fully planned and audited mount list, computed before the container is created; agentbox applies mounts and starts the agent in one step, so "applied" means "planned and hashed". `project_root` is the owner's repository root; agentbox clones it onto a session branch and mounts the clone, not the owner's checkout. Registered inputs the agent needs are reachable through the tool server, never through the mount.

Launcher to Toulmin (`tm session confirm S4 --json FILE` before the agent starts):

```json
{"handshake":1,"session":"S4","preset":"analysis-offline","isolation":"container",
 "network":"locked","lists_applied":true,"mounts_hash":"sha256:...",
 "container_paths":{"socket_dir":"/run/agentbox/toulmin","bridge":"/run/agentbox/bin/tm-bridge","project_root":"/home/tim/proj"},
 "clone":"/home/tim/.local/share/agentbox/projects/proj/clones/S4","branch":"agentbox/S4","base":"<sha>",
 "agent":"claude","launcher":"agentbox <version>"}
```

End (launcher to Toulmin, `tm session end S4 --json FILE`):

```json
{"handshake":1,"session":"S4","clone":"/home/tim/.local/share/agentbox/projects/proj/clones/S4",
 "base":"<sha>","head":"<sha>","exit":"agent_exit","ended_at":"2026-11-10T19:35:57Z"}
```

`clone`, `branch`, and `base` are present in clone mode and absent in checkout mode (a non-git root or a project that opted out), in which case Toulmin labels the session `owner checkout mounted` and reads the agent's commits from `project_root`. `exit` is one of `agent_exit`, `signal`, `kill`, `gc`, or `error`. Toulmin computes patch IDs from the clone (or `project_root`) between `base` and `head`. agentbox's teardown runs the same path on agent exit, on its own signals, from `ab session kill`, and from `ab session gc`, and it calls `tm session end` with values recorded at session start plus the head from its end-of-session scan, so Toulmin accepts an end message whose `head` equals `base` (no commits), one that arrives long after the session (a `gc` run), and one in checkout shape after a clone-mode confirm (`exit: error` before the clone was made), which it labels `ended before clone`. The full contract, including what agentbox returns to prove it, is `docs/toulmin/agentbox_toulmin_integration.md` in the agentbox repository. Unisolated sessions produce the confirm message with `isolation: unisolated` and `preset: none` from `tm session start --unisolated`. `tm session serve` refuses to start before the confirm event exists, so a running tool server is always a confirmed one. The launcher that backgrounded `serve` kills it at `tm session end`; `tm session close` signals the process ID the server writes beside its socket; agentbox does not restart a dead per-session child today, so if a login node kills the tool server, `tm status` lists the open session whose socket is dead with the command `tm session serve S<n>` for the owner to run (or the launcher, once it supervises). A session that ends without a confirm event (a launcher that crashed between `start` and `confirm`) is labeled `isolation unknown` and treated as `unisolated` for every label and warning.

#### 11.15 Action file (static view to `tm apply`)

`{"actions":1,"exported_at","chain_head":"sha256:...","items":[{"op":"draft.promote","targets":["S4.12@<revision id>"],"allow_unapproved":false},{"op":"run.approve","code_version":"..."},{"op":"claim.review","ref":"S4.12","revision":"sha256:...","outcome":"confirmed","note":"..."}]}`. `tm apply` refuses an item whose target changed since `chain_head` (`E8101`), prints one line per item with its target, revision, and any override, requires `--yes-override` for any item that sets an override, and asks for confirmation. Review items carry the revision they were made against.

#### 11.16 Event bodies (gate A types)

Every body is an object whose values use the 11.6 typed-value encoding for table-derived values, and plain JSON strings for handles, IDs, paths, and enumerations. Keys listed in `{"a","b"}` form. `handle` means a short identifier (`F7`, `F7@v2`, `C31`), `id` means an event ID. Handles are `F<n>`, `F<n>@v<m>`, `C<n>`, and `R<n>`, with `n` and `m` decimal integers from 1 without leading zeros; `S<n>.<m>` refs are gate B. The format of a report handle is not fixed in gate A (it is decided before P6), so a `report` value is any non-empty string. A body of a known type and `tv` has exactly the listed keys; a missing or extra key, or a value not of the form this section, 11.6, 11.7, 11.8, 11.9, or 11.23 states for it, is `E1002` at its pointer; a value whose form is stated nowhere is accepted as any JSON value in gate A. The gate A type set is exactly the list below; `events/mod.rs` exposes no other writable variant in gate A.

| Type (`tv`) | Body keys | Notes |
| --- | --- | --- |
| `project.created` (1) | `{"project_id","owner_id","record_format","repo_root_hash"}` | `repo_root_hash` is the SHA-256 of the repository root path string, so a moved checkout is visible |
| `settings.changed` (1) | `{"key","value","previous"}` | `value` and `previous` are typed values or `null` (unset); a list-valued setting uses the `list` typed value of 11.6, and a `set` replaces the whole list |
| `file.registered` (1) | `{"file","kind","path","title","description","source"}` | `kind` is `input` in gate A |
| `file.version_added` (1) | `{"file","version","fingerprint","origin","size","snapshot","content_label","manifest"}` | `origin` is `{"kind":"registered"\|"run"\|"changed_outside_run","run":handle or null}`; `content_label` is one of 6.2's three values or `null` for a first version; `manifest` is the directory manifest object or `null` |
| `file.checksum_checked` (1) | `{"file","version","algorithm","matched","mismatches"}` | `mismatches` is a list of relative paths |
| `file.parse_spec_set` (1) | `{"file","spec","spec_fingerprint"}` | `spec` is the 11.7 object |
| `file.kept` (1) | `{"version","object","compressed_size","reason","fingerprint_only"}` | `object` is `sha256:<fingerprint of raw bytes>`; `reason` is `promotion` or `owner` |
| `file.marked_removable` (1) | `{"file"}` | |
| `file.restored` (1) | `{"file"}` | |
| `file.renamed` (1) | `{"file","from","to"}` | |
| `file.panels_set` (1) | `{"version","panels"}` | `panels` is a list of names |
| `report.registered` (1) | `{"report","path"}` | No spans: a report's spans are working-tree state that changes after registration, so the `spans` table is scan-derived (11.17), not folded from this event |
| `claim.added` (1) | `{"ref","kind","sentence","report","cited","citations","steps","assertion","qualifiers","supersedes"}` | `cited` is a list of version handles; `citations` a list of 11.23 citation objects; `steps` and `assertion` the 11.8 and 11.9 objects or `null`; `supersedes` a ref or `null`; `kind` is `quantitative`, `qualitative`, or `mixed` (item 83) |
| `claim.checked` (1) | `{"ref","revision","against","evidence","assertion","steps","final_rows_hash","key_map_hash","key_map","sql","versions","error"}` | `key_map` is the list of input key tuples in final-row order, the list `key_map_hash` digests, written in full and never truncated, so a claim over a large final table makes a large event; `revision` is the event ID of the revision checked; `against` the version handles; `evidence` is `{"outcome","detail"}`; `assertion` is `{"outcome","observed","expected"}` or `null`; `steps` a list of `{"op","rows_out","description"}`; `versions` is `{"parse_specs":{handle:fingerprint},"translator","duckdb","extensions","rules","build"}`, where `build` is `musl` or `glibc`; `error` is `{"code","message"}` or `null`; outcomes are `holds`, `fails`, `not_checkable`, `error` |
| `claim.revised` (1) | `{"ref","sentence","cited","citations","steps","assertion","qualifiers","reason"}` | Every revision restates the claim in full, so `cited` and `citations` are present even when unchanged and the fold never reads an earlier revision; `reason` is `update_text` in gate A |
| `claim.reviewed` (1) | `{"ref","revision","outcome","note","report_fingerprint"}` | `outcome` is `confirmed`, `disputed`, or `question` |
| `claim.retracted` (1) | `{"ref","reason"}` | |
| `claim.superseded` (1) | `{"ref","replacement"}` | written in the same batch as the replacement's `claim.added` |
| `audit.completed` (1) | `{"report_hash","since","counts"}` | `counts` is `{"files_rehashed","mismatches","verdicts_rerun","verdicts_changed","open_flags"}` |
| `chain.torn_tail_truncated` (1) | `{"segment","offset","bytes"}` | |
| `lock.taken_over` (1) | `{"previous","epoch"}` | `previous` is `{"host","pid","epoch","heartbeat"}` |

Two objects appear inside several bodies:

| Object | Keys | Notes |
| --- | --- | --- |
| `snapshot` | `{"size","mtime","mtime_ns","ctime","ctime_ns","ino"}` | `size`, `mtime_ns`, `ctime_ns`, and `ino` are integers; `mtime` and `ctime` are RFC 3339 UTC strings with their sub-second part in the `_ns` field (11.1). No device number (item 26) |
| `manifest` | `{"fingerprint","ignore","entries"}` | `entries` is a list of `{"path","snapshot","fingerprint"}` sorted by `path` in 11.10 order; `ignore` is the exclude list actually applied; `fingerprint` is `sha256:` of the canonical JSON of `entries` |

Reserved for later gates and not declared in gate A (no variant in `events/mod.rs`): the `run.`, `session.`, `viewer.`, `journal.`, `draft.`, `receipt.`, `engine.` namespaces and `file.tracked`, `file.retired`, `file.hashing_finished`. A line of one of these types is read as opaque under 11.11 and cannot be written.

#### 11.17 Index schema

SQLite, one database per machine and project (3.4). All tables are rebuildable from the chain. `schema.sql`:

```sql
CREATE TABLE head (chain TEXT PRIMARY KEY, last_id TEXT NOT NULL, seq INTEGER NOT NULL,
  segment INTEGER, offset INTEGER, batch INTEGER, epoch INTEGER, schema_version INTEGER NOT NULL);
CREATE TABLE events (id TEXT PRIMARY KEY, chain TEXT NOT NULL, seq INTEGER NOT NULL, type TEXT NOT NULL,
  segment INTEGER NOT NULL, offset INTEGER NOT NULL, at TEXT NOT NULL);
CREATE INDEX events_chain_seq ON events(chain, seq);
CREATE TABLE opaque_events (id TEXT PRIMARY KEY, type TEXT NOT NULL, tv INTEGER NOT NULL);
CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT, event_id TEXT NOT NULL);
CREATE TABLE files (file TEXT PRIMARY KEY, kind TEXT NOT NULL, path TEXT NOT NULL, title TEXT, description TEXT,
  source TEXT, level TEXT NOT NULL, retired INTEGER NOT NULL DEFAULT 0, removable INTEGER NOT NULL DEFAULT 0,
  agent_denied INTEGER NOT NULL DEFAULT 0, registered_event TEXT);
CREATE INDEX files_path ON files(path);
CREATE TABLE file_versions (version TEXT PRIMARY KEY, file TEXT NOT NULL, n INTEGER NOT NULL, fingerprint TEXT NOT NULL,
  origin_kind TEXT NOT NULL, origin_run TEXT, content_label TEXT, size INTEGER, kept_object TEXT,
  fingerprint_only INTEGER NOT NULL DEFAULT 0, event_id TEXT NOT NULL);
CREATE INDEX file_versions_file ON file_versions(file, n);
CREATE TABLE parse_specs (file TEXT PRIMARY KEY, spec TEXT NOT NULL, spec_fingerprint TEXT NOT NULL, event_id TEXT NOT NULL);
CREATE TABLE panels (version TEXT NOT NULL, name TEXT NOT NULL, PRIMARY KEY (version, name));
CREATE TABLE reports (report TEXT PRIMARY KEY, path TEXT NOT NULL, event_id TEXT NOT NULL);
CREATE TABLE spans (report TEXT NOT NULL, ref TEXT NOT NULL, line INTEGER NOT NULL, sentence TEXT NOT NULL,
  inline_code INTEGER NOT NULL DEFAULT 0, section_kind TEXT, PRIMARY KEY (report, ref, line));
CREATE TABLE claims (ref TEXT PRIMARY KEY, id TEXT NOT NULL, kind TEXT NOT NULL, state TEXT NOT NULL, -- added, reviewed, retracted, superseded (gate A)
  created_event TEXT NOT NULL, current_revision TEXT NOT NULL, report TEXT, supersedes TEXT, superseded_by TEXT);
CREATE TABLE claim_revisions (event_id TEXT PRIMARY KEY, ref TEXT NOT NULL, sentence TEXT NOT NULL, cited TEXT NOT NULL,
  citations TEXT NOT NULL, steps TEXT, assertion TEXT, qualifiers TEXT, at TEXT NOT NULL);
CREATE INDEX claim_revisions_ref ON claim_revisions(ref, at);
CREATE TABLE claim_cites (ref TEXT NOT NULL, version TEXT NOT NULL, PRIMARY KEY (ref, version));
CREATE TABLE claim_keys (ref TEXT NOT NULL, version TEXT NOT NULL, key_tuple TEXT NOT NULL, PRIMARY KEY (ref, version, key_tuple));
CREATE TABLE claim_events (ref TEXT NOT NULL, event_id TEXT NOT NULL, type TEXT NOT NULL, at TEXT NOT NULL, PRIMARY KEY (ref, event_id));
CREATE TABLE audits (event_id TEXT PRIMARY KEY, at TEXT NOT NULL, since TEXT, report_hash TEXT NOT NULL, counts TEXT NOT NULL);
CREATE TABLE verdicts (event_id TEXT PRIMARY KEY, ref TEXT NOT NULL, revision TEXT NOT NULL, against TEXT NOT NULL,
  evidence_outcome TEXT NOT NULL, assertion_outcome TEXT, body TEXT NOT NULL, at TEXT NOT NULL);
CREATE INDEX verdicts_ref ON verdicts(ref, at);
CREATE TABLE reviews (event_id TEXT PRIMARY KEY, ref TEXT NOT NULL, revision TEXT NOT NULL, outcome TEXT NOT NULL, note TEXT, at TEXT NOT NULL);
CREATE TABLE flags (subject_kind TEXT NOT NULL, subject TEXT NOT NULL, name TEXT NOT NULL, reason TEXT NOT NULL,
  since_event TEXT NOT NULL, PRIMARY KEY (subject_kind, subject, name));
```

JSON-valued columns (`spec`, `cited`, `citations`, `steps`, `assertion`, `body`, `counts`, `key_tuple`) hold canonical JSON text; `verdicts.body` is the whole `claim.checked` body, so `claims.show` reads SQL, steps, hashes, versions, and error from it. `IndexTx` lives in `tm-index` (4.1) and wraps one `rusqlite::Transaction`, exposing `execute(sql, params)`, `query_rows(sql, params) -> Vec<Row>`, and `head(chain) -> Head`; every library's fold and the Core depend on that crate, and no library depends on the Core. The Core writes `head`, `events`, `opaque_events`, `settings`, `audits`, and `claim_events`; each library's fold writes its own tables. **Fold contract:** one transaction per main-chain batch; a fold first looks the event ID up in `events` and returns if present, which is the idempotence check, and may otherwise insert or update rows in its tables; a fold never reads the record directory, the working tree, or an earlier revision of the event's subject; a full rebuild drops every table except `stat_cache` and `spans` and replays from the first segment. `stat_cache(path TEXT PRIMARY KEY, snapshot TEXT, fingerprint TEXT, confirmed_at TEXT)` and `spans` live in the same database but are not rebuildable from the chain and are never dropped: the stat cache is node-local confirmations, and `spans` is rewritten from the working tree by `tm report add`, `tm report check`, and `tm status`, each of which rescans every registered report with the 11.22 scanner. No event carries spans, because a span is written into the report after the claim it points at exists. `claims.evidence` recomputes per-step rows by re-running the Checker on the parse cache for the pinned versions; `claims.history` reads `claim_events`; `claims.citing_rows` reads `claim_keys`, which the Claims fold fills from each `claim.checked` body's `key_map`, replacing every row for that `(ref, version)` pair on each re-check, while the author's citations are stored as given and never rewritten; `audit.last` reads `audits`. `files.list` text search is `LIKE` over `title`, `description`, and `path`; the cited count is `SELECT COUNT(*) FROM claim_cites JOIN file_versions USING (version) WHERE file = ?`.

#### 11.18 Checker semantics

##### 11.18.1 Expression grammar (`filter.where`, `derive.expr`)

```
expr     := or
or       := and ( "or" and )*
and      := not ( "and" not )*
not      := "not" not | cmp
cmp      := add ( ( "=" | "!=" | "<" | "<=" | ">" | ">=" ) add )?
add      := mul ( ( "+" | "-" ) mul )*
mul      := unary ( ( "*" | "/" ) unary )*
unary    := "-" unary | primary
primary  := literal | column | call | "(" expr ")"
call     := ( "abs" | "log" | "log2" | "log10" | "exp" | "coalesce" ) "(" expr ( "," expr )* ")"
literal  := INT | DEC | STRING | "true" | "false" | "missing"
column   := IDENT | "`" any-but-backtick+ "`"
INT      := DIGIT+                            (typed value int; unsigned)
DEC      := DIGIT+ "." DIGIT+                 (typed value dec; unsigned, no exponent form)
STRING   := '"' ( any-but-quote-or-backslash | "\\" '"' | "\\" "\\" )* '"'
IDENT    := [A-Za-z_][A-Za-z0-9_]*
```

Whitespace between tokens is ignored. Literals carry no sign: `-` is always an operator, unary or binary, so `a -5` is `a - 5` and a negative constant is unary minus applied to an unsigned literal. `coalesce` takes two or more arguments; every other call takes one. The words `and`, `or`, `not`, `true`, `false`, and `missing` are reserved, lower case only, and need backticks as column names. An `IDENT` followed by `(` is a call, and a call to a name the `call` rule does not list is a syntax error; any other `IDENT` is a column, so a column may be named `log`. A call with the wrong number of arguments, an unterminated string or backtick name, a backslash in a string followed by anything but `"` or `\`, an empty backtick name, a chained comparison (`a < b < c`), and any other token sequence the grammar does not derive are syntax errors, `E6006`.

##### 11.18.2 Evaluation rules

| Rule | Statement |
| --- | --- |
| Where | `filter.where` and `derive.expr` are evaluated in Rust over the Arrow batch with `bigdecimal` (the reference checker uses Python `decimal` with the same context); the relational operations (`sort`, `take`, `rank`, `group`, `join`, `select`) run in DuckDB over the result. The stored SQL covers the relational part and names the derived columns as inputs. |
| Types | Operands are typed values (11.6); a `float64` column is `dec`. `int` combined with `dec` gives `dec`. `+`, `-`, and `*` on two `int` give `int`; `/` always gives `dec`; unary `-` and `abs` keep `int` or `dec`; `log`, `log2`, `log10`, and `exp` take `int` or `dec` and give `dec`; `coalesce` gives its arguments' common type, `int` and `dec` widening to `dec`, and its arguments otherwise share one type. Arithmetic on any other type is `E6004`. `=` and `!=` take two numbers, two `str`, two `bool`, two `date`, or two `datetime`; `<`, `<=`, `>`, and `>=` take the same pairs except `bool`; a `date` and a `datetime` do not compare. `and`, `or`, and `not` take `bool`. The literal `missing` has no type and fits any operand position; a `derive` whose result has no type is `E6004`. Comparisons give `bool`. |
| Precision | Decimal arithmetic follows the General Decimal Arithmetic specification (IEEE 754-2008 decimal) with a context of 34 significant digits and half-even rounding: `bigdecimal` on the Rust side, Python's `decimal` on the reference side, both named in 11.6. Results of `log`, `log2`, `log10`, `exp`, `mean`, and `median` are rounded to 15 significant digits before they become `dec` values (see the `log` family row). |
| Missing | Any arithmetic or comparison with a `missing` operand is `missing`, and `nan` is `missing` for every operator. `and`, `or`, and `not` are SQL three-valued: `true or missing` is `true`, `false and missing` is `false`, `false or missing` and `true and missing` are `missing`, and `not missing` is `missing`. A `filter` keeps a row only when `where` is `true`; `missing` and `false` drop it. A `where` that evaluates to anything but a boolean or `missing` is `E6004`. |
| Domain | Division by zero, `log` of a value at or below zero, and `exp` overflow give `missing`, not an error. Decimal overflow beyond 34 digits gives verdict `error` with `E5002`. |
| Strings | `=` and `!=` compare NFC-normalized bytes. `<`, `<=`, `>`, `>=` on strings use 11.10 binary order. Comparing a string with a number, bool, or date is a validation error `E6004` type mismatch, raised before DuckDB runs. |
| Dates | `date` and `datetime` compare chronologically; arithmetic on them is `E6004`. |
| `log` family | `log` is the natural logarithm, translated to DuckDB `ln` where DuckDB sees it. `log`, `log2`, `log10`, and `exp` take and return `dec` through IEEE double, as do `mean` and `median`. These six are the only places two conforming implementations may differ, because the Rust side takes them from the `libm` it is linked against (musl's in the static build, the host's in the glibc fallback) and the reference checker takes them from CPython's, which can differ in the last bit on one machine. The difference is absorbed rather than tolerated: every result of these six is rounded to 15 significant digits before it becomes a `dec`, which is below the 16 to 17 digits at which a one-unit-in-the-last-place difference appears, so `final_rows_hash` is comparable for every pipeline and there is one comparison rule. A claim therefore cannot assert the 16th significant digit of a logarithm or a mean. |
| `sort` | `by` entries apply in order; `missing` rows go first or last per `missing`; the table key is appended ascending as the final tie-breaker. |
| `take` | Keeps the first `n` rows of the current total order; requires a preceding `sort` or `rank`. |
| `rank` | `scope: all` ranks the whole table; `scope: group` requires `group_by` (a non-empty column list) and ranks within each group. `missing: first` or `last` assigns ranks as if sorted that way; `missing: exclude` drops rows whose `by` value is missing. `ties: min`, `max`, and `dense` follow SQL `RANK`, inverse `RANK`, and `DENSE_RANK`; `ordinal` is `ROW_NUMBER` ordered by `by` then the key. The rank column is named `rank` and the table order becomes rank ascending then key. |
| `group` | Output rows are one per distinct `by` tuple, ordered by the tuple in sort order. Output columns: the `by` columns, each aggregate under its `as`, `member_count` (int), and `member_keys_hash` (`sha256:` of the canonical JSON list of the members' key tuples sorted in 11.10 order). `count` counts every member, missing included; `sum`, `min`, and `max` ignore missing and give `missing` when every member is missing; `mean` and `median` are DuckDB `DOUBLE` rounded to 15 significant digits and converted to `dec` per 11.6. Types: `count` gives `int`; `sum` takes `int` or `dec` and keeps it; `min` and `max` take any type but `bool` and keep it; `mean` and `median` take `int` or `dec` and give `dec`; any other column type is `E6004`. The output key is the `by` tuple. |
| `join` | `on` pairs left columns with right columns; both sides must be keyed and the pairs must cover the right key. Every right column, its key columns included, is added with the prefix `<right>.`. Each `on` pair must compare under the `=` rule of the Types row (`E6004`). `inner` drops unmatched left rows; `left` keeps them with `missing`. The output key is the left key. |
| `select` | Keeps the listed columns and the key columns. |
| Key mapping | Each final row carries the input key tuple(s) it derives from: the left key for joins, the `by` tuple for groups, the row key otherwise. `key_map_hash` is `sha256:` of the canonical JSON list of those tuples in final-row order. |
| Result hash | `final_rows_hash` is `sha256:` of the canonical JSON list of the final rows, in final order, each row an object of column name to typed value, columns in the order the last step produced them. |

##### 11.18.3 Assertion rules

| Kind | Rule |
| --- | --- |
| `value` | Locate the final row by `row` (key values); absent row is `fails`. Compare the cell with `expected` under `op` and `tol`. |
| `rank` | The row's 1-based position in the final order; absent row is `fails`. Compared with `expected` under `tol` as an `eq`. |
| `count` | Final row count compared with `expected` under `op`. |
| `compare_rows` | Both cells located as for `value`; either absent is `fails`. |
| `exists` | `holds` if the row is present, `fails` otherwise. |
| `op` on strings | `eq` and `ne` only, by NFC bytes; other operators and `tol` are `E6004`. |
| `op` on booleans | `eq` and `ne` only; other operators and `tol` are `E6004`. |
| `op` on dates | Chronological; `tol` is `E6004`; a `date` and a `datetime` do not compare (`E6004`). |
| Mixed types | `int` and `dec` compare as decimals; any other pair of different types is `E6004`. |
| Tolerance | With `m = abs + rel * |b|`, where `b` is `expected` or, for `compare_rows`, the right cell, computed exactly without rounding, and `m = 0` with no `tol`: `eq` holds when `|a - b| <= m`; `ne` when `|a - b| > m`; `lt` when `a < b + m`; `le` when `a <= b + m`; `gt` when `a > b - m`; `ge` when `a >= b - m`. |
| `between` | `expected` is `[lo, hi]`, inclusive; holds when `lo - (abs + rel * |lo|) <= a <= hi + (abs + rel * |hi|)`. The bounds are not reordered, so `lo > hi` holds for nothing beyond the tolerance. |
| Missing | A `missing`, `nan`, or `inf` cell never satisfies any comparison and gives `fails` (11.9). |

##### 11.18.4 Description templates

One sentence per step, produced by the Checker and reproduced byte for byte by the reference checker. Placeholder renderings: `{n}`, `{m}`, and `{k}` are integers with `,` as the thousands separator; `{expr}` is the parsed expression re-serialized from the parse tree, never echoed from the author's text: one space around every binary operator and around `and` and `or`, one space after `not`, no space after unary `-`, no space inside parentheses, one space after each comma in a call, column names as written with backticks kept only where the grammar requires them, literals as written, and parentheses emitted only where precedence requires them; `{cols}` and every other list is comma-separated with `, and ` before the last item when there are three or more and ` and ` for two; `{dir}` is `ascending` or `descending`; `{missing}` is `placed first`, `placed last`, or `excluded`; `{ties}` is the `ties` value as written (`min`, `max`, `dense`, `ordinal`); `{scope}` is `all rows` or `each {group_by}`; `{key}` is the table key column list; `{fn}` is the aggregate name as written; `{right}` is the right table's name as registered.

| Op | Template |
| --- | --- |
| `filter` | `Keep rows where {expr} (missing values excluded); {n} of {m} rows remain.` |
| `select` | `Keep columns {cols}; {n} rows.` |
| `sort` | `Sort by {col1} {dir1}, then {col2} {dir2} (missing values {missing}; ties broken by {key}); {n} rows.` |
| `take` | `Take the first {k} rows; {n} rows remain.` |
| `rank` | `Rank by {col} {dir} within {scope} (ties: {ties}; missing values {missing}); {n} rows.` where `{scope}` is `all rows` or `each {group_by}` |
| `derive` | `Add column {as} = {expr}; {n} rows.` |
| `group` | `Group by {cols} and compute {as1} = {fn1}({col1}), ...; {n} groups from {m} rows.` |
| `join` | `{Inner\|Left} join with {right} on {left_col} = {right_col}, ...; {n} rows.` |

#### 11.19 Settings

All settings are owner-set through `tm settings set KEY VALUE` and recorded by `settings.changed`. Unknown keys are `E7001`; values failing validation are `E7002`. The index `settings` table is the read path; a chain scan is the fallback when the index is absent.

| Key | Type | Unit | Default | Validation | Gate |
| --- | --- | --- | --- | --- | --- |
| `hashing.inline_threshold` | int | MiB | 64 (OQ1) | 1 to 65536 | A |
| `hashing.threads` | int | threads | 1 | 1 to 256 | A |
| `hashing.bandwidth_mib` | int | MiB/s | 0 (no cap) | 0 or more | A |
| `hashing.trust_ctime` | bool | | true | none | A |
| `lock.heartbeat_seconds` | int | s | 5 | 1 to 60 | A |
| `lock.takeover_seconds` | int | s | 300 (OQ2) | 30 to 86400 | A |
| `keep.warn_mib` | int | MiB | 1024 (OQ5) | 1 or more | A |
| `keep.budget_mib` | int | MiB | unset | 1 or more, or unset | A |
| `checker.memory_limit_mib` | int | MiB | 2048 | 256 or more | A |
| `checker.temp_directory` | str | path | the parse cache directory | absolute path | A |
| `checker.call_seconds` | int | s | 30 (OQ4) | 1 or more | A |
| `view.rows_per_page` | int | rows | 500 | 1 or more | A |
| `manifest.ignore` | list of str | glob | `.snakemake/`, `.nextflow/`, `__pycache__/`, `*.swp`, `.DS_Store` | each entry parses as a glob | A |
| `login_node.pattern` | str | regex | unset | valid regex | C |
| `approval.require_all` | bool | | false | none | C |
| `run.late_output_wait` | int | s | 600 | 0 or more | C |
| `run.finish_login_limit_mib` | int | MiB | 1024 | 1 or more | C |
| `session.abandon_hours` | int | h | 72 (OQ3) | 1 or more | B |
| `session.rows_per_call` | int | rows | 10000 (OQ4) | 1 or more | B |
| `session.bytes_per_call_mib` | int | MiB | 64 (OQ4) | 1 or more | B |
| `session.calls` | int | calls | 5000 (OQ4) | 1 or more | B |
| `session.receipt_grace` | int | s | same as `run.late_output_wait` | 0 or more | C |
| `harness.env_vars` | list of str | | `AGENTBOX_SESSION`, `CLAUDECODE` (OQ7) | none | A |
| `files.agent_denied` | list of str | path | empty | paths inside the repository | B |

Gate A implements the gate A rows; the others are accepted by `settings.set` with validation from this table but read by nothing until their gate.

#### 11.20 Static view pages

Written by `tm view --out DIR`; `tm audit --out DIR` writes into `DIR/audit/` so the two never collide. Every page embeds its source data as `<script type="application/json" id="tm-data">` holding one object keyed by query name (with the parameters used as a nested `params` object), each value the exact JSON that read-interface query returned, which the G9 parity test compares with the CLI's `--json` output. Pages are plain HTML with one shared `style.css`, no external resources, no inline event handlers, and the viewer's Markdown sanitizer for every owner- or agent-written string. Gate A page set:

| Path | Queries | Sections |
| --- | --- | --- |
| `index.html` | `files.list`, `claims.list`, `reports.list`, `flags.list`, `audit.last`, `settings.show` | counts, open flags with next commands, current settings, links |
| `reports/<report>.html` | `reports.spans`, `claims.list` (by report) | spans in file order with ref, recorded sentence, current text, drift state; beside each span, that claim's current verdict, assertion outcome, and open flags from `claims.list` |
| `claims/<ref>.html` | `claims.show`, `claims.evidence`, `claims.citing_rows`, `claims.history`, `files.lineage` (per cited version) | sentence, kind, qualifiers; cited rows highlighted in full rows with neighbors, capped at `view.rows_per_page` with the number not shown stated and `tm claim evidence --json` named as the way to read the rest; per-step output and description; assertion and verdict with version block; flags with clearing actions; reviews; other claims citing these rows; analysis tree (gate A: the registered input itself as the only node); review tick boxes |
| `files/<file>.html` | `files.show`, `claims.list` (by cited version) | title, description, source; versions with fingerprints and labels; parse spec; claims citing each version |
| `flags.html` | `flags.list` | every open flag grouped by kind |
| `audit.html` | `audit.last` | the last audit summary with a link to its report |

Review tick boxes on claim pages feed the shared `static/actions.js`, which assembles an 11.15 action file holding `claim.review` items only and offers it as a download named `actions.json`. `tm apply` in gate A refuses `draft.promote` and `run.approve` items with `E8102` action not available in this gate.

#### 11.21 Main-chain write sequence and object store

**Lock-time `HEAD` write.** Taking or breaking the lock rewrites `HEAD` with the incremented epoch and every other field unchanged, by the same write-then-rename sequence as step 3 below.

**Batch write.** (0) If the open segment's length plus the batch would exceed 64 MiB, seal the open segment (see Sealing) and start a new one; a batch never spans two segments, so a single batch larger than 64 MiB is a segment of its own. (1) Serialize the events, each as one line ending in `\n`; `len` is the byte count of all event lines including their newlines; `h` is `sha256:` of exactly those bytes. (2) Write the header line (terminated by `\n`, not counted in `len`, not covered by `h`) and the event bytes to the open segment with one `write` call per line, then `fsync` the segment. (3) Write `HEAD` to `main/HEAD.tmp`, `fsync`, rename to `main/HEAD`, `fsync` the `main/` directory. `HEAD.offset` is the byte length of the open segment after step 2. `seq` increases by one per event across batches; `HEAD.seq` is the last event's `seq`. **Sealing** is done by the writer that would overflow: it compresses `seg-NNNNNN.jsonl` to `seg-NNNNNN.jsonl.zst` at zstd level 9, `fsync`s it, re-reads and verifies every batch in the packed copy, then removes the plain file. Readers accept either form.

**Objects.** Path `objects/sha256/<first two hex>/<remaining hex>.zst`, addressed by the fingerprint of the raw bytes. Keep: stream the source through SHA-256 and zstd (level 3) into `objects/tmp/<random>`, compare the computed fingerprint with the expected one (`E3203` on mismatch), `fsync`, rename into place, `fsync` the directory.

#### 11.22 Span grammar and drift normalization

A span is `[` text `]{` attributes `}` on one logical line of the report; text may not contain an unescaped `]`, and `\]` escapes one. Attributes are whitespace-separated; the first must be `.claim`; `ref=<ref>` is required; `<ref>` matches `C[0-9]+` or `S[0-9]+\.[0-9]+`; other attributes are stored as given and not interpreted. A span inside a fenced code block (lines between ```` ``` ```` or `~~~` fences) is not a span. A span whose text contains inline code (backticks) is recorded with `inline_code` true, and its drift check is `not checkable`. Two spans with the same ref in one report are both recorded (one claim, two locations). A heading class is `{.<kind>}` at the end of an ATX heading line; the scanner records the kind of the nearest preceding heading for each span as `section_kind` (in the `spans` table), used only for display. Normalization for drift, in order: NFC; replace every run of Unicode whitespace with one space; trim.

---

#### 11.23 Citation objects

One JSON object per citation, stored in `claim.added` and `claim.revised` bodies, in `claim_revisions.citations`, and in the conformance envelopes. Key values use the 11.6 typed-value encoding typed per the cited table's parse spec. The CLI spelling in item 19 is parsed into exactly these objects.

| Kind | Keys | Notes |
| --- | --- | --- |
| `cells` | `{"kind":"cells","version","keys","columns"}` | `version` is a version handle; `keys` is a list of key tuples, each a list of typed values in the spec's key-column order; `columns` is the list of cited column names, or `null` for the whole row |
| `whole_file` | `{"kind":"whole_file","version"}` | |
| `page` | `{"kind":"page","version","page"}` | `page` is a 1-based int |
| `panel` | `{"kind":"panel","version","panel"}` | `panel` is a name registered by `file.panels_set` |
| `quote`, `page_region`, `image_region` | `{"kind",...}` | Reserved (11.13); stored as given and not interpreted; a claim carrying one is `not checkable` for that citation |

CLI spelling: `F7@v2` is `whole_file`; `F7@v2#<column>=<value>` with one `#<column>=<value>` per key column in the spec's key order is one `cells` key tuple (repeat the whole handle for more tuples, or give a file with `--cited-keys`); `F7@v2#page=3`; `F7@v2#panel=B`. Inside a value, `\#`, `\=`, and `\\` escape the three special characters; values are parsed per the column's type. When the cited version has a parse spec, every `#<column>=<value>` segment names a key column, the segments name every key column exactly once and in key order, and `page` and `panel` are ordinary column names; without a parse spec the only forms are the bare handle, `#page=<n>` with `n` a decimal integer from 1 without leading zeros, and `#panel=<name>` with a non-empty name. A value is read in the 11.6 `v` form of its column's type (`true` or `false` for `bool`; a `float64` column as `dec`). The CLI spelling has no column selector, so the `cells` citation it produces has `columns: null`. Any other input, an unknown escape included, is `E6007` with `field` `null`. In the JSON form, `keys` is a non-empty list and each tuple a non-empty list of typed values, none `missing`, `nan`, `inf`, or `list`; `columns` is `null` or a non-empty list of column names; `page` is a JSON integer of at least 1; `panel` is a non-empty string; an absent, extra, or unknown member is `E6002` and a wrong JSON type `E6004`.

### 12. Command map (`tm <object> <verb>`)

| Object | Verbs | Notes |
| --- | --- | --- |
| `tm init` | `--record PATH`, `--attach PATH` | Creates the record, `.toulmin/project.json`, and the record binding under `~/.config/toulmin/`; `--attach` binds an existing record to this checkout or machine |
| `tm status` | `--refresh`, `--hash`, `--stale`, `--pending`, `--flags` | Reads in, scans, lists flags with next commands; `--stale`, `--pending`, and `--flags` are `staleness.list`, `pending.list`, and `flags.list` |
| `tm file` | `register` (`--defer-hash`), `hash`, `show`, `list`, `lineage`, `keep`, `removable`, `restore`, `rename`, `panels`, `versions`, `rows` | `versions` and `rows` are `files.show` and `files.rows` |
| `tm table` | `spec`, `preview` | Parse spec; `preview` is `files.rows` without a claim |
| `tm analysis` | `declare`, `show`, `list`, `edit` | `edit` creates a new code version |
| `tm run` | `<analysis>` (the wrapper), `again`, `finish`, `ingest`, `approve` (`--pending` lists every unapproved code version once for bulk approval), `review`, `show`, `list`, `diff`, `--from-draft` | The wrapper never takes the lock; `diff` is `runs.diff_since_approved` |
| `tm session` | `start`, `serve`, `confirm`, `end`, `close`, `list`, `show`, `rejections` | `start --role writer` (the only role in V1); `start --unisolated --harness NAME`; `serve` and `confirm` are called by the launcher |
| `tm draft` | `list`, `show`, `promote`, `send-back` | Operates on journal items by ref |
| `tm claim` | `add`, `show`, `list`, `evidence`, `history`, `citing-rows`, `check`, `review`, `update-text`, `retract`, `supersede`, `accept` | Owner claims get `C<n>`; `citing-rows` is `claims.citing_rows` |
| `tm report` | `add`, `spans`, `check` | Drift check |
| `tm view` | `--out DIR`, `--serve` | Static and live |
| `tm apply` | `FILE` | Confirms and commits an action file |
| `tm audit` | `--since LAST`, `--out DIR`, `last` | Full validation; `last` is `audit.last` |
| `tm doctor` | | Environment checks |
| `tm lock` | `status`, `break` | |
| `tm settings` | `show`, `set`, `unset` | |
| `tm upgrade` | `duckdb`, `checker` | Deliberate, followed by the changed-verdict report |

A day's work on a cluster: `tm status`; edit a script; `sbatch` a script containing `tm run A3`; `tm status` after the job; `tm session start`; agent works; `tm session end`; `git merge`; `tm audit`; `tm view --out review/`; review in the browser; `tm apply review/actions.json`; `tm draft send-back S4.7 --note "..."`; `tm status`.

## Alternatives Considered

Each alternative lists what it would have given (for), what it costs (against), and the outcome with the reason. Grouped by the part of the design it touches.

### Process and writers

| Alternative | For | Against | Outcome and reason |
| --- | --- | --- | --- |
| A long-lived central service as the only writer | One process owns the record; agents and the viewer share it; no read-in step | Login nodes kill long processes; users land on different login nodes; compute nodes cannot reach a login-node socket; the service runs commands in the wrong environment; `tm run -- sbatch` returns before outputs exist | Rejected. Replaced by the no-daemon writer model |
| One lease shared by every writer (CLI, tool servers, viewer, wrapper) | One chain, no journals, promotion needs no cross-reference | A writer holding the lease blocks the CLI; taking it per write makes thousands of segments; takeover compares clocks across hosts; a paused writer can fork the chain | Rejected. Per-writer journals plus a CLI-only main chain with epoch fencing |
| Owner-only actions enforced by an owner Unix socket with `SO_PEERCRED` and namespace checks | Kernel-verified caller identity | Needs a daemon to own the socket; rootless containers share the owner's UID; shared loopback on login nodes | Rejected. No daemon; owner actions are a CLI path plus the `OwnerCap` type; the viewer uses a confirmation step |
| Owner actions committed from static pages | One review surface | Static pages cannot write; any writable endpoint reintroduces the browser attack surface | Rejected. Static pages produce an action file that `tm apply` confirms |
| Live viewer takes the main-chain lock for its own writes (considered at drafting, D27) | Immediate commit from the browser | Widens rule 5 to a second writer of the main chain; the CI dependency check must cover the viewer's lock code | Rejected for V1. Viewer writes its launch journal; the next locked `tm` command commits |
| Live viewer shells out to `tm apply` (D27) | One code path for static and live | Depends on the `tm` binary on the path; inherits the non-interactive label from D23 | Rejected for the same reason |
| `O_EXCL` lock on one viewer journal, or a read-only second viewer (D26) | Fewer journals | Needs heartbeat and takeover for a dead viewer, or blocks review from a second login node | Rejected. One journal per viewer launch |

### Record and formats

| Alternative | For | Against | Outcome and reason |
| --- | --- | --- | --- |
| SQLite as the record (event table with previous-row hashes) | One file, queryable, transactional | Unsafe on network filesystems (NFS, Lustre, GPFS locking); corrupts silently | Rejected. SQLite is only the rebuildable index on local disk |
| Git-like record internals (refs, merges, one file per record) | Familiar model; branching for drafts | Inode limits on clusters; merge complexity; one file per event is slow on network storage | Rejected. Append-only segments plus journals |
| Report text stored as Toulmin events | Toulmin owns the whole report | Duplicates git; agents would need a Toulmin write path for prose; merging prose is git's job | Rejected. Markdown in git with Pandoc spans; the owner merges agent text like code |
| Content-hash prefixes as claim refs in spans (`ref=c3f9a2`) | No allocator | Hash changes on revision; a prefix can collide; unreadable | Rejected. Session-scoped refs `S4.12` and `C31` are unique without a central allocator |
| Promotion-time display aliases (`C12` assigned at promote) | Short owner-facing numbers | Two names per claim; spans written before promotion cannot carry the alias | Rejected. One ref per claim, assigned at creation |
| An outside anchor for the hash chain (git note pushed to a remote the agent cannot reach) | Detects a full rewrite | The stance does not defend against deliberate rewrites; adds a remote dependency and credentials handling | Rejected. The chain detects accidental damage; the isolation table says so |
| Signing in V1 | Attributable checkpoints | An unisolated agent can read the owner's keys, so it adds little; collaborators (who would verify) are V2 | Deferred to V2 |
| Export and import in V1 | Round-trip test of the record format | `tm audit` plus copying the record directory covers the testing need | Deferred to V1.x |
| A faster hash than SHA-256 (BLAKE3) | Faster hashing of large data | Not measured to be the bottleneck; provider checksums are MD5 or SHA-256; algorithm prefix leaves room | Deferred until measured |

### Runs, staleness, and files

| Alternative | For | Against | Outcome and reason |
| --- | --- | --- | --- |
| Run every analysis in a container in V1 | Enforced reads; reproducible environment | Scheduler-submitted jobs run outside the head process's container; needs a lot of machinery | Deferred to V1.x (agentbox presets, Landlock, or an observed mode with `strace` or reprozip) |
| A Snakemake adapter reading Snakemake's own records in V1 | Per-rule lineage for free | Ties V1 to one workflow tool; declared runs with snapshot checks cover the need | Deferred to V1.x |
| DVC as the run and staleness layer | Declared stages, content fingerprints, carried-forward outputs, and dependency-ordered staleness already exist | Python runtime and pip install on clusters without admin rights; SQLite state under `.dvc/tmp` on network filesystems; no two-phase receipts from compute nodes; no agent attribution; its own cache duplicates the kept-files folder | Rejected. Toulmin borrows the stage format; reading `dvc.lock` is a V1.x adapter |
| Toulmin owns sandboxing (the earlier 15-module spec's sandboxing module, M12) | One project to install | Duplicates agentbox; mounts and backups are a launcher's job | Rejected. Two-way handshake only |
| A full hash of every run output on every run | Simple rule; always confirmed | Re-hashes unchanged terabytes after a one-file rerun; I/O priority has no effect on network filesystems | Rejected. Hash only new or changed entries; `tm audit` does the full validation |
| Record agents' test runs | Visibility into agent exploration | Agents test inside their own sandbox; receipts from there would be unverifiable | Dropped |
| Approval required for every run | Every output reviewed | Approval fatigue; duplicates run review; every job-array task would need one | Rejected. Required only for code with agent-written or unattributed changes; carries over by code version |
| Separate file-version accept and claim accept | Owner controls when an edited input counts | Two accept steps for one edit; hand edits are already labeled | Rejected. Hand-edited inputs are versioned automatically; only claims are accepted |
| Auto-accept claims onto new versions as an opt-in | Less clicking after reruns | Hides changes the owner should see | Removed from V1 |
| Agent-registered inputs as an opt-in | Agents can bring data in | No safe registration path from inside the sandbox; a path the agent names in the working tree could be a symlink out of it | Deferred to V1.x. Agents name files in draft notes and the owner registers them |

### Checker and claims

| Alternative | For | Against | Outcome and reason |
| --- | --- | --- | --- |
| PRQL as the step language | Exists, readable, compiles to DuckDB SQL, prefix execution shows each step | Pre-1.0; no documented missing-value placement in sort; a second moving layer between the claim and the SQL | Rejected. Toulmin's own versioned JSON operations; a text front end may come later |
| Ibis or Substrait as the step layer | Mature relational IR | Same second-layer problem; Substrait is heavier than eight operations need | Rejected with PRQL |
| Qualitative claims with no system check | Simpler model | An evidence-exists check is reproducible and catches reruns that remove cited rows | Rejected |
| Fuzzy expected-versus-observed image check | Some check on figures | A person judges it anyway; not reproducible, so it cannot be a system check | Rejected. An image claim is a qualitative claim with a note on what to expect |
| Detecting unlinked numbers in claim text | Catches unchecked numbers | False positives; some hard numbers are read off figures | Deferred to V1.x. Reviewers send weak claims back |
| Automatic conflict detection between claims | Reports contradictions | Needs semantics Toulmin does not have | Replaced by "other claims citing these rows" in the evidence view |
| A separate profiles module in V1 | Clean place for field rules | Only one profile ships | Rejected. General rules live in the Checker until RNA-seq arrives |
| Exploration count per session | A signal about cherry-picking | No reliable meaning across prompts, models, and tasks | Dropped. The session diagnostic log stays |
| Deferring the Python reference checker | Less work at gate A | It is the only independent check on the Checker's meaning for inputs nobody anticipated | Rejected. Built at gate A by a separate agent session from the spec alone |
| A tilde pin on DuckDB and a glibc-linked build | Easier builds | Patch releases change floating-point sums; `libm` differs across cluster systems | Revised. Exact pin, extensions locked, static musl build |

### Language

| Alternative | For | Against | Outcome and reason |
| --- | --- | --- | --- |
| Python | The earlier spec's language; the owner's daily language; fast to write | No closed enums or exhaustive matching; null everywhere; static distribution to clusters without admin rights is painful | Rejected for Rust |
| Go | Static binaries; simple concurrency | No sum types; DuckDB binding is third-party; MCP SDK less mature | Rejected for Rust |
| TypeScript | Official MCP SDK | Needs a Node runtime on clusters; no static binary | Rejected for Rust |

## Risks and Mitigations

### Assumptions

| Assumption | If false |
| --- | --- |
| git is installed where `tm init`, `tm session end`, and `tm run` run, and the project is a repository | `tm init` refuses; a compute node without git produces a run with no commit or diff, labeled `git unavailable` (`E4202`) |
| agentbox implements its side of the handshake | Sessions can still be unisolated; the container level is unavailable |
| Tables cited by cells have a unique single or composite key | The parse spec is refused and the table is citable only as a whole file |
| The record directory is on a filesystem compute nodes can see | Receipts go to the project inbox and are read in later |
| The owner fetches agent session branches from the reported clone path, merges them with git, and removes the clone | Report text from agents stays on the session branch; claims can still be promoted |
| DuckDB builds and runs under musl | Confirmed in the P0 spike (DuckDB 2.0.0, bundled, under musl with mimalloc and an 8 MiB stack; `ln`, `exp`, and `avg` bit-identical to a dynamically linked glibc 2.17 build run on CentOS 7, and identical between hosts for the static build). Fallback is the `cargo-zigbuild` glibc 2.17 target; cross-machine maths differences are possible there because `log` and `exp` come from the host's `libm`, and `tm audit` on each machine reports any verdict that differs from the stored one |

### Risks

Security threats are in the threat table (5.3); capacity limits are in the Non-Functional Requirements budgets.

| Risk | Mitigation |
| --- | --- |
| The index rebuild exceeds 60 s at one million events | Folds are batched per segment and measured in CI on a generated chain; the budget is a gate A exit criterion |
| Clock skew between nodes mislabels receipts or hides an edit | Run checks compare snapshots with each other, never with a clock. The racy rule compares a file's mtime with the mtime of a marker file the wrapper writes to the same filesystem immediately before each snapshot, so both times come from one clock. `run in progress` and `interrupted` use the receipt's own `node.started`, and the flag reason says the comparison is clock-dependent; `tm doctor` reports skew |
| Login nodes kill the tool server or viewer | Both are restartable: the launcher reruns `tm session serve`, the tool server recovers from its journal, and `tm status` names dead sockets; the viewer gets a new launch number |
| Staleness noise from directory inputs | Exclude lists, the default ignore list, and the narrowing suggestion in `tm status` |
| Owner approves code without reading | Out of scope by the stance; the diff with agent lines marked is on the approval page |
| The lock holder dies mid-batch or mid-keep | Torn-tail rule (3.1): readers stop before the partial batch and the next locked command truncates it on the chain; keep is write-then-verify-then-rename into `objects/`, so a partial object is never addressed |
| Interleaved appends on a mount that fails `tm doctor` | Segment length is checked against `HEAD.offset` before every append; a mismatch is reported as a fork and `tm audit` names it (3.1) |
| A receipt arrives for a declaration the owner has since edited | The receipt names its declaration; read-in checks against that one and labels the run `declaration changed during run` (11.5) |
| Two sessions revise one ref | Order is read-in then `seq`; a revision of an already promoted ref is read in as rejected (section 7, D29) |
| The index or stat cache lands on a network filesystem | Node-local defaults; `tm doctor` refuses a network filesystem for them; integrity check on open (3.4) |
| A tool server replies before its journal line is durable | Every journal line is synced before the reply; the index and `journal.head_recorded` carry the last line ID (3.2) |
| Output hashing exceeds the job's remaining walltime | Partial receipt on `SIGTERM`; `tm run finish` prints a resubmit line when the recorded hashing estimate exceeds the login-node limit; the guide recommends padding walltime by the recorded hashing time (CUJ 2) |
| A viewer-confirmed action is committed against a changed target | `viewer.action_confirmed` records the chain head and revision IDs; read-in re-applies the `tm apply` checks and records a rejection on failure (3.2) |

## Decisions Register

One row per decision. Status is Settled unless noted. "Replaces" names the earlier recommendation or Summary.md decision the row supersedes. D1 to D25 are closed with the owner in the brief; D26 to D29 were decided with the owner at drafting time to close gaps found in D15 and D16, and appear in no earlier input.

Shorthand used in the Reason and Replaces columns: B1 to B6 are the blockers and M1 to M10 the major findings of the brief's first review (`concepts_brief.md`); "adopt as written" is that brief's list of second-review items folded in without owner judgment; M12 is the sandboxing module of the earlier 15-module spec; "the hypothesis table" is the writer table in `toulmin_hypothesis.md` decision 1 and "the modules table" is the event ownership table in `toulmin_modules.md`.

### Numbered decisions

| ID | Decision | Status | Reason | Replaces |
| --- | --- | --- | --- | --- |
| D1 | No central daemon. The CLI writes the main chain under a short lock; each agent tool server, viewer launch, and the run wrapper writes its own file | Settled; refined by D15, D25 | Login-node rules, compute-node runs, and user environments rule out a service | B3 (long-lived service does not fit clusters): a leased on-demand service |
| D2 | Viewer reads, records reviews, and commits owner actions only through a confirmation step; promotion and approval otherwise CLI only | Settled; amended by D18 and D27 | Browser attack surface and shared loopback | B1 (owner-only by connection): an owner Unix socket |
| D3 | No outside anchor and no signing in V1; wording stated per isolation level | Settled | The threat stance does not defend against deliberate rewrites | B2 (record detects only accidental damage): an outside anchor at each promotion |
| D4 | Registration makes a file citable; bytes are copied on first citation by a promoted claim; drafts pin fingerprints only | Settled | Unreviewed copies could fill a cluster quota; cycle between Files and Claims | Summary: "cited files are copied and never removable" |
| D5 | New versions: re-check writes a verdict against the new version; the claim stays until the owner bulk-accepts; identical content is carried forward; no auto-accept | Settled | Two readings of "re-checked automatically"; plot libraries embed dates | Summary: auto-accept as an opt-in |
| D6 | Run check by before-and-after stat snapshots, never a clock; git commit recorded; `carried forward` label; blocking head process | Settled | Incremental workflow reruns, `cp -p`, NFS attribute caching | "Every output created or changed during the run" |
| D7 | Agent code attribution from the launcher's commit list at session end, matched by SHA then patch ID; each run records its git commit | Settled | Commit trailers are forgeable; patch IDs survive rebases | Attribution at branch fetch (earlier spec) |
| D8 | Query steps are Toulmin's own fixed set of versioned JSON operations, translated to explicit DuckDB SQL | Settled | PRQL is pre-1.0 with undocumented null placement | PRQL recommendation |
| D9 | Record is append-only JSON event segments plus journals, an inbox, and a content-addressed kept-files folder; SQLite is only an index | Settled | SQLite is unsafe on network filesystems | SQLite event table recommendation |
| D10 | Reports are Markdown in git with Pandoc spans and heading classes; claims stay in the record | Settled | Reuses git for drafts and merging | Report text as records |
| D11 | Five synchronous libraries, two shared pure crates, a Core, and three front ends; libraries never call each other | Settled | Each part testable alone; coding agents can build one part at a time | Nine-module list |
| D12 | Profiles folded into the Checker; conflict detection replaced by "other claims citing these rows"; agent-registered inputs deferred; session diagnostic log outside the record; agents can draft declarations | Settled | Scope | Separate profiles module, conflict detection, agent-input opt-in |
| D13 | Owner correction and action semantics: review, promote, send back, approve, retract, supersede, update text, accept | Settled | A promoted typo had no fix; promote, review, approve, and send back were not told apart | Undefined owner actions |
| D14 | Rust, with async only at the edges, pedantic Clippy, no `unwrap` in libraries, `unsafe` only in the DuckDB binding, `cargo-deny` and `cargo-audit` | Settled | Closed enums and exhaustive matching suit many states; official `rmcp` and `duckdb-rs` | Python (earlier spec) |
| D15 | Writer model: per-writer journals with their own chains; CLI-only main chain with `O_EXCL` lock and epoch fencing; read-in records journal heads | Settled; gaps closed by D26 to D28 | One lease for every writer blocks the CLI or makes thousands of segments | D1's shared lease |
| D16 | Claim refs are session-scoped (`S4.12`, `C31`), assigned at creation, kept across revisions; the ID is the creation event's hash; supersede gives a new ref | Settled; gap closed by D29 | Content-hash IDs change on revision; no central allocator exists | Hash-prefix refs with promotion-time aliases |
| D17 | Approval is a review of a code version (script and support-script fingerprints plus command template); required for agent-written or unattributed changes; carries over to reruns unless the rerun's run check fails; bulk approval; promotion gate with recorded override | Settled | Approval fatigue; approval duplicated run review | Approval per run |
| D18 | Static view early and permanent; live viewer last | Settled | Evidence must be readable before the viewer exists; unreliable networks | Live viewer only |
| D19 | Hashing tiers: stat-only routine checks with a small-file threshold; `tm run` hashes only new or changed entries; `tm audit` hashes everything; one thread in batch jobs; bandwidth cap | Settled | A one-file rerun must not re-hash a 500 GB folder | Full hash on every run output; I/O priority |
| D20 | Build gates A (record, checker, claims, static view, audit), B (agents), C (runs and staleness), D (live viewer) | Settled | Agents are tested before cluster machinery; each gate is usable alone | Summary's six-stage order |
| D21 | Python reference checker kept, built at gate A by a separate agent session from the spec alone; the six IEEE-double functions are reconciled by 11.6's 15-significant-digit rounding rather than by a comparison tolerance, so the differential suite expects exact equality | Settled | Only independent check on the Checker's meaning | Deferring it |
| D22 | Staleness belongs to an analysis with plain reasons; identical content (bytes, keyed table, decompressed content) stops the spread; command templates; hand edits are new versions automatically; failed runs do not make outputs current; lineage per version | Settled | Staleness noise from compressed outputs and run-specific command values | Staleness per run; separate file accept |
| D23 | Agents outside agentbox get `tm session start --unisolated --harness NAME`; owner actions record terminal interactivity; a known harness variable triggers confirmation and a label | Settled | Unisolated is the likely cluster default; visibility, not prevention | Undetected unisolated agents |
| D24 | DuckDB pinned exactly with a lock file; extensions locked; static musl build with a faster allocator; glibc 2.17 fallback; smoke tests on CentOS 7, Rocky 8 and 9; musl spike first. Outcome recorded in `docs/decisions/musl-spike.md` (2026-11-10): musl confirmed, fallback built with `cargo-zigbuild`, no `libstdc++` | Settled | Cross-machine verdicts must match | Tilde pin, glibc build |
| D25 | A shared stateless filesystem-facts component; the run wrapper is a Core operation that never takes the lock; the Core owns staleness and permissions | Settled | The wrapper broke the module rules and duplicated hashing | Wrapper inside Runs |
| D26 | One viewer journal per launch (`V<n>`), numbered under the lock | Settled at drafting | Two viewer launches would otherwise fork one journal | D15's single viewer journal |
| D27 | Viewer-confirmed owner actions are written to the launch journal and become main-chain events with `source: viewer` when the owner commits them (`tm apply --viewer`, or an interactive `tm status`); pending until then and shown as such | Settled at drafting | Rule 5 (only the CLI writes the main chain) and the hypothesis table said journal; the modules table said main chain | Modules table row "owner action via viewer: main chain" |
| D28 | Receipts read from the project's fallback inbox while any session is open are labeled `possibly agent-issued`, and the label carries to the run, its outputs, and claims citing them | Settled at drafting | The project inbox is a project-side path an agent can write to during an unisolated session, and a clone carries a committed `.toulmin/` | Unlabeled fallback inbox |
| D29 | Any session may propose a revision of any unpromoted or sent-back ref; the current revision is the latest promoted one; the promote event names the exact revision event; order across journals is read-in then `seq`; late revisions of promoted refs are rejected | Settled at drafting | Two journals can hold revisions of one ref with no order but read-in | D16's unstated cross-session rule |

### Summary.md decisions: kept, changed, or dropped

| Summary.md decision | Outcome | Where it stands now |
| --- | --- | --- |
| Requires a git repository; works alongside git | Kept | Assumptions; glossary |
| Fingerprint each run's script, inputs, outputs, and declared support scripts; copy only files flagged important | Changed | Copy on first citation by a promoted claim, or on purpose (D4); "flag important" became `tm file keep` and agent title proposals |
| Claims cite only registered files (inputs or tracked outputs) | Kept | Section 6 |
| Stable title, description, and source per registered file | Kept | G8 |
| Uncited files hidden by default with a plain count | Kept | Computed "cited"; `files.list` filter |
| A cited file its script no longer produces is retired; claims highlighted | Kept | `retired` label and `cited file retired` flag; restore added |
| Steps declared one at a time, translated to a standard query language | Changed | Toulmin's own JSON operations to explicit DuckDB SQL (D8); PRQL rejected |
| Quantitative, qualitative, and mixed claims | Kept | Section 7 |
| Numbers in claim text not detected in V1 | Kept | Deferred to V1.x |
| New version: re-check and flag; owner decides; auto-accept as opt-in | Changed | No auto-accept in V1 (D5) |
| First version personal; sharing in V2 with the design ready | Kept | Reserved fields; owner ID |
| One instance per user, no accounts | Kept | One record directory per project, random owner ID |
| Works without admin rights; agents on a separate host | Changed | Separate host and separate user deferred to V1.x; container (network `locked`, `packages`, or `open`) and unisolated in V1 |
| Record always tamper-evident; owner signs at checkpoints | Changed | No signing in V1 (D3); tamper evidence is limited to accidental damage and partial edits, stated per isolation level |
| No required workflow tool or layout | Kept | Non-goals |
| Good practice by default; weaker options owner-only, recorded, shown | Kept | Section 5.5 |
| Cloud models allowed with warning and per-dataset controls | Changed | Toulmin never talks to a model; the control is the agent-denied list and the launcher's network mode; the warning is in `tm session start` output |
| Agent queries logged; no exploration count | Kept | Per-session diagnostic log outside the record |
| Export in the project's own format wrapped in a research-packaging standard | Dropped from V1 | Export and import V1.x; RO-Crate wrapper V2; PROV terms in run events keep the mapping cheap |
| Separate parts behind defined interfaces with shared tests | Kept | Section 4 and the Verification Plan |
| Linux first; permissive code license, public-domain specification | Kept | Unchanged |
| Scope items: fuzzy image check, conflict detection, export and import, PDF text-layer and web-page evidence, RNA-seq profile, field profiles, agent roles | Dropped or deferred | Image check rejected; conflict detection replaced; export and import, PDF and web text, number detection, profiles to V1.x; one agent role in V1 |
| Six-stage build order | Changed | Four gates (D20), agents before cluster machinery |

## Future Improvements

Each build gate after A gets its own design document, scoped by this one.

- **Gate B design document: agents.** The agent tool server (`tm-agent`) and its MCP tool schemas derived from section 10; the stdio bridge; the agentbox handshake and `tm session start|serve|confirm|end|close`; unisolated sessions and the D23 labels; session journals and the read-in of proposals; refs `S<n>.<m>` and counter recovery; `tm draft promote` and `tm draft send-back` with D29; `tm apply`; per-session limits and the diagnostic log; the must-not-mount and agent-denied lists. Exit: an agent session drafts claims and text on registered inputs and the owner promotes them (G5 measure).
- **Gate C design document: runs and change.** The run wrapper as a Core operation; receipts, inbox, quarantine, and the project fallback inbox with D28; run checks and labels; lineage per version; staleness (section 8); new versions, re-checks, and `tm claim accept`; approval by code version, carry-over, and bulk approval; attribution from patch IDs; run groups for job arrays; `tm run again` and `finish`; drafted declarations and `tm run --from-draft`; `tm doctor` cluster checks. Exit: the full analysis tree on every claim page (G4, G6, G7, G10 measures).
- **Gate D design document: live viewer.** `tm view --serve`, launch journals `V<n>` (D26), the confirmation step and pending actions (D27), token, Host, and Origin checks, sanitized Markdown, content security policy, and parity with the static view. Exit: interactive review with owner actions committed through the confirmation step.
- **V1.x:** PDF and web text extraction with quote checks; agent-registered inputs with an upload tool; export and import; field profiles (RNA-seq first); number detection in claim text; enforced or traced runs; Snakemake and Nextflow adapters for per-rule lineage; format-aware comparison (BAM `@PG`, PDF dates); a text front end for query steps; agents as a separate Unix user or on a separate host; a faster hash if measured to be needed.
- **V2:** signing, collaborators, restriction levels, and the RO-Crate packaging wrapper.
- **Later:** model checks as a third line of evidence, labs, and an open format standard.

### Items held back from gate A

The gate A plan omits the following, which the gate B, C, and D documents pick up.

Gate B: `tm-agent` and `tm-bridge` crates, binaries, release jobs, and the standalone owner-cap builds; `events/{session,journal,draft}.rs`; `handshake.rs`, `action_file.rs` beyond `claim.review`; `journals/` layout and `journal.rs`; `S<n>.<m>` parsing; the D29 cross-journal order; the session actor variant; the journal half of `read_in.rs`; `apply.rs` for `draft.promote`; `read/sessions.rs`, `read/pending.rs`; OQ3 and OQ4 readers; `static_view_apply` beyond reviews.

Gate C: `events/{runs,receipt,engine}.rs`; `receipt.rs`, `declaration.rs`; `inbox/` layout and `inbox.rs`; `recheck.rs`; the receipt half of `read_in.rs`; `engine_upgrade.rs` and `tm upgrade`; `read/{analyses,runs,staleness}.rs`; `staleness.rs`; analyses and runs pages and the lineage tree beyond the registered input; `docs/user/clusters.md`; `claim.accept`; `file.retired` and `tm file restore`'s retired case.

Gate D: `events/viewer.rs`; the viewer actor variant; `OwnerCap` drop after `viewer.launch`.

### agentbox reconciliation (2026-11-10)

Read against the launcher's code (`session.py`, `mounts.py`, `hardening.py`, `presets.py`, `paths.py`, `sockguard.py`, the Claude and OMP adapters). Confirmed as is: the must-not-mount list is enforced through the launcher's protected-path check with a per-session extension; the launcher keeps per-session background children with pid files and stops them at teardown; every preset runs rootless podman as the owner's UID with `--cap-drop=ALL`, `--read-only`, and no new privileges; egress always goes through a per-session proxy. Changes agentbox has agreed to make before gate B: a clone-on-session-branch mode (replacing the bind mount of the owner's checkout and its rsync backup for git projects), a launcher-level session-mount list with audit, per-session MCP entries for both harnesses with the token in a `0600` file, `AGENTBOX_AGENT` in the environment, and the `tm session end` callout from teardown. Not yet built and recorded as gaps: mid-session restart of a dead child; a `podman create` then `start` split, which is why `mounts_hash` covers the planned list. Not offered and not assumed: a network-off mode, an unisolated mode, a separate-user mode. Built on 2026-11-10 on the agentbox branch `toulmin-gate-b` (commits 2bc1d16 and 7424f0c, 1,270 unit tests green against a stub `tm`), with these deviations accepted: clone mode refuses the dev podman tier and fails closed (use `--podman none` or `observe`); the `serve` pid file lives in the session run directory; the grant scan runs over the owner's tree, not the clone; `mounts_hash` uses sorted-key compact JSON, equal to RFC 8785 for these inputs; `must_not_mount` is checked against mount sources with containment for literals and ancestor match for globs. `claude --mcp-config` is accepted at the pinned Claude Code 2.1.318 and the `.claude.json` merge is used. Still owed from a host with podman and OMP: the in-container view and `podman inspect` grep (`tests/integration/test_toulmin.py`) and the OMP environment probe (`make omp-env-probe`); OMP's `.mcp.json` format is assumed Claude-compatible until that probe runs.

### Gate exit criteria

Each gate exits when the measures below, drawn from the Goals table, pass.

| Gate | Exit criteria (goal measures) | Usable for |
| --- | --- | --- |
| A | G1 (no claim shown as holds with a non-holds check, on the reference project); G2 (schema rejects every missing-choice case, both checkers); G3 (every static claim page has evidence, step outputs, and a tree whose leaves are registered inputs or labeled gaps, with the tree stub in gate A showing the registered input itself); G8; G9 for the gate A queries (CLI and static view agree, with the MCP path added in B); G11; G12 (smoke on three distributions; the batch-job half is gate C); G13; G14; the index budget; the musl spike decision recorded in `docs/decisions/musl-spike.md`; the owner has completed `fixtures/ACCEPTANCE.md` on one published paper (its claims entered with `tm claim add` and reviewed in the static view) | Hand-written claims on registered tables |
| B | G5 (CI dependency check; every MCP tool refuses every owner-action payload; no owner-action event in any journal); G9 extended to the MCP path; G15 for the unisolated session; D26 to D29 scenario tests (two viewer launches never share a journal, a viewer-confirmed action is pending then committed, two sessions revising one ref produce one promoted revision); CUJ 3 and 4 as scenario tests | Agents drafting claims and text on registered inputs |
| C | G4; G6; G7; G10; G12 batch-job half (receipt from an `sbatch` job on a compute node); G15 for undeclared inputs and unapproved code; D28 scenario test (project-inbox receipt during a session is labeled); CUJ 2 and 7 as scenario tests, with the CUJ 2 test covering the `SIGTERM` path through a real `exec` wrapper; `tm audit` reports verdict changes after a DuckDB upgrade | The full analysis tree |
| D | G3 for the live viewer (parity with the static view on every page); owner actions through the confirmation step with token, Host, Origin, and CSP tests; a CSRF and a DNS-rebinding test fail to commit an action | Interactive review |

--------------------------------------------------------------------------------

## Non-Functional Requirements

### Budgets

| Item | Budget | How checked |
| --- | --- | --- |
| Index rebuild | Under 60 s for one million events on a laptop-class disk | CI on a generated chain; gate A exit criterion |
| Incremental index update | Under 1 s for a batch of 100 events | CI |
| `tm status` disk scan | Stat only; under 5 s for 100,000 tracked entries on a warm cache; large stat-changed files listed as unconfirmed, not hashed | Scenario test with a generated tree |
| Hashing in `tm run` | Only new or stat-changed entries; one thread by default in batch jobs; optional bandwidth cap; hashing time recorded on the receipt; the user guide recommends padding walltime by the previous run's recorded hashing time | Receipt field; scenario test that a one-file rerun hashes one file |
| Checker memory | DuckDB `memory_limit` and `temp_directory` set explicitly per call; defaults 2 GiB and the parse cache directory; a setting raises them | Snapshot test of the settings SQL |
| Agent limits | Per-session rows, bytes, calls, and time (OQ4) | Tool server refuses with `E6103` |
| Login-node warnings | `tm run` on a login node (detected by hostname pattern or a setting) warns; `tm run again` for a batch-recorded run prints a resubmit line instead of running | Scenario test |
| Kept bytes | Compressed with zstd; size warning above the threshold (OQ5); optional project budget | `file.keep` path |

### Portability

One static musl binary per architecture (x86_64 first), with mimalloc as the global allocator (confirmed in the P0 spike) because DuckDB's own benchmarks show large slowdowns with musl's allocator, and the real entry point run on a spawned thread with an 8 MiB stack because musl's default main-thread stack is too small for DuckDB's planner. DuckDB is compiled in (bundled, with Parquet and the Arrow table function; no other extension). Fallback: a dynamically linked glibc build produced with `cargo-zigbuild --target x86_64-unknown-linux-gnu.2.17` and `-g0`, which needs glibc 2.17 or newer and no `libstdc++` at all. A CentOS 7 devtoolset build is not used: it still links `libstdc++.so.6` at the gcc 4.8 ABI level, which DuckDB refuses without `DUCKDB_CUSTOM_PLATFORM`; the recipe is kept in `docs/decisions/musl-spike.md` as the known-good alternative if the zig path fails. CI checks symbol versions and smoke-tests both binaries on CentOS 7, Rocky 8, and Rocky 9. The bridge is a separate small static binary because it runs inside sandboxes. No runtime dependencies beyond git on the path.

### Determinism

DuckDB pinned to an exact version with a committed lock file; extension auto-install and auto-load off; Parquet compiled in; one DuckDB thread per check; stable sort with the key as the last tie-breaker; explicit null placement and binary collation in every generated SQL; decimal arithmetic in `derive` except `log` and `exp`. The static musl binary carries its own `libm`, so `log` and `exp` are reproducible across machines for that build; the glibc fallback takes them from the host's `libm`. Measured on 2026-11-10 against glibc 2.17 on CentOS 7, DuckDB's `ln`, `exp`, and `avg` over DECIMAL and DOUBLE were bit-identical to the musl build; glibc 2.28 and later use a different implementation and have not been compared, which is why 11.6 rounds every result of `log`, `log2`, `log10`, `exp`, `mean`, and `median` to 15 significant digits: that is below the digit at which a one-unit-in-the-last-place difference shows, so the reference checker, which takes these functions from CPython's `libm`, produces the same strings and the same hashes. Every verdict records parse-spec fingerprint, translator version, DuckDB version, loaded extensions, rule version, and build flavour (`musl` or `glibc`). A DuckDB upgrade, a checker upgrade, or a change of build flavour is a deliberate `tm upgrade` followed by a changed-verdict report, and `tm audit` after it.

### Security and privacy

Owner actions require `OwnerCap`; `tm-agent` and `tm-bridge` are separate binaries that cannot construct one (the constructor is feature-gated to `tm-cli`, and CI builds both standalone with it off). `tm view --serve` drops its `OwnerCap` after `viewer.launch`, since every later viewer write goes to the launch journal. Sessions authenticate by token on a Unix socket in a per-session directory under the owner's `0700` `/tmp/toulmin-$UID/`, path under 107 bytes. The live viewer binds loopback only, requires a per-launch token, checks Host and Origin, sanitizes Markdown, sets a strict content security policy, and commits owner actions only with a one-time code. Agent-denied files are refused by the tool server and listed in the must-not-mount set. Outside content is marked as data in tool responses. The record directory, the record binding under `~/.config/toulmin/`, index, stat cache, session diagnostic logs, viewer tokens, and other sessions' sockets and journals are on the must-not-mount list. No telemetry; no network access by any Toulmin process except the loopback viewer and the local socket.

### Rollout and rollback

There is no server to roll out. A new `tm` binary reads any older record format it knows; unknown event types are opaque; a higher record format version refuses writes and says which binary version is needed. Rolling back a binary is copying the old file back; the record directory is append-only, so nothing is lost. DuckDB upgrades are rolled back by reinstalling the previous binary; verdicts record which engine produced them.

## Additional Resources

- `concepts_brief.md` (Nov 9, 2026): decision history and review findings.
- `toulmin_hypothesis.md` (version 3) and `toulmin_modules.md` (version 3): the authoritative design this document expands.
- `Summary.md` (Nov 9, 2026): problem, users, and principles.
- Prior art named in the brief: DVC stage format (declarations), Jupyter's token model (viewer), git's racy-clean rule and stat cache, Maildir (inbox), W3C PROV (run events), in-toto link metadata (receipts), RFC 8785 (canonical JSON), Frictionless Table Schema (parse spec shape), RO-Crate (V2 packaging).

--------------------------------------------------------------------------------

## Detailed Implementation Plan (build gate A)

Gate A delivers: the musl spike; `tm-formats` and `tm-fsfacts`; `tm-record` (main chain, lock, objects; the journal and inbox APIs are gate B and C); `tm-owner-cap`; `tm-files` for inputs; `tm-checker` with its test suite and the Python reference checker; `tm-claims` with report spans and drift; `tm-core` with the gate A operations, index, read interface, static view, and `tm audit`; and the `tm` binary. Usable for hand-written claims on registered tables, such as a published paper's tables. No sessions, journals, receipts, runs, or live viewer; the formats they need arrive with their gates, which 11.11's opaque-event rule makes safe.

Repository layout is a Cargo workspace at the repository root. Paths are relative to it. Every crate has `Cargo.toml`, `src/lib.rs` (or `main.rs`), and `README.md`, listed once here and not repeated. Every path below is created unless marked otherwise.

### Phases and checkpoints

Each phase ends when its checkpoint command passes on CI. No phase starts before the previous checkpoint. Every item appears in exactly one phase; item 93 is split by file across P1, P2, P4, and P6.

| Phase | Builds | Checkpoint |
| --- | --- | --- |
| P0 | Items 1-10: workspace, toolchain, CI and release workflows and scripts (jobs whose inputs do not exist yet are no-ops until their phase), spike, decision record | First, `spike/musl-duckdb` runs its fixed query under musl on CI; then one change lands item 10 (`docs/decisions/musl-spike.md` with the chosen build path), deletes the spike, and updates item 1's `mimalloc` and target settings to match |
| P1 | Items 11-25, 88, 90, 93 (`formats.md`), 94: `tm-formats`, conformance cases, Python formats cross-check, generated error catalog | `cargo test -p tm-formats` and `python3 reference/formats/test_formats.py` pass with identical hashes for every case |
| P2 | Items 26-42 (including 26a, 26b, 30a), 91, 93 (`record.md`): `tm-fsfacts`, `tm-duckdb`, `tm-index`, `tm-record`, record conformance | `cargo test -p tm-fsfacts -p tm-record` passes; every tampering case in `conformance/record/tamper/` is reported by name |
| P3 | Items 43-52, 82a: `tm-owner-cap`, `tm-files`, the workspace fixture tables | `cargo test -p tm-files` passes on `fixtures/` |
| P4 | Items 53-62, 92, 93 (`checker.md`): `tm-checker`, snapshots, properties, fuzz targets; owner review of `checker.md`. Item 92's `expected.json` files come first, from a separate agent session reading `checker.md` and the case inputs only, before any Rust Checker code is written | `cargo test -p tm-checker` passes; `cargo insta test -p tm-checker` reports no pending snapshots; the owner has signed off `docs/spec/checker.md` and item 92's `expected.json` files (recorded in `docs/decisions/checker-spec-review.md`) |
| P5 | Items 87 and 89: Python reference checker by a separate agent session from `checker.md` and the conformance cases only; generator and differential script | `python3 reference/checker/refcheck.py conformance/checker` matches every expected file; `python3 reference/checker/validate.py conformance/formats/steps/invalid conformance/formats/assertions` rejects every invalid case with the named code and field; `scripts/differential.sh 10000` reports no difference in any field on both runner classes |
| P6 | Items 63-71, 93 (`claims.md`): `tm-claims` | `cargo test -p tm-claims` passes, including the span fixtures |
| P7 | Items 72-78, 80, 81: `tm-core` without the view | `cargo test -p tm-core` unit tests pass; `scripts/ci/bench-index.sh` under 60 s |
| P8 | Items 79, 82-86, 95-97: static view, CLI, scenarios, docs, smoke | `cargo test -p tm-core --test scenarios` and `cargo test -p tm-cli` pass; `scripts/ci/smoke.sh` passes on the three distributions; the owner has re-derived one paper's claims (item 82's acceptance checklist signed) |

### Target files

#### 0. Workspace, build, and CI

1. `Cargo.toml`. Workspace manifest listing all crates. `[workspace.dependencies]` with exact pins, to be filled at P0 from the versions current on that date and committed: `duckdb` (`=1.20000.1`, `default-features = false`, features `bundled`, `parquet`, `vtab-arrow`, giving DuckDB 2.0.0; the `arrow` pin is the major version duckdb-rs re-exports), `arrow`, `parquet` (cache writer), `serde`, `serde_json`, `sha2`, `rusqlite` (bundled), `zstd`, `bigdecimal` (34-digit arithmetic; `rust_decimal` is capped at 28 digits), `unicode-normalization`, `rustix` (`statx`, `O_EXCL`, `fsync`), `lopdf` (PDF page tree only), `pulldown-cmark` plus `ammonia` (Markdown render and sanitize), `askama` (HTML templates), `clap`, `mimalloc` (confirmed by P0), `proptest`, `insta`, `assert_cmd`, `tempfile`, `libfuzzer-sys`. `[profile.release]` with `lto`, `codegen-units = 1`, `panic = "abort"`. Rationale: one place for exact pins (D24); every dependency chosen here, none by the implementer.
2. `Cargo.lock`, committed. It is empty until the first crate exists; P1's first build locks the pinned versions, and item 4's allowed-crate list is re-checked against it then. Rationale: D24.
3. `rust-toolchain.toml`. Pinned stable toolchain (version filled at P0) and the `x86_64-unknown-linux-musl` target; a `rust-toolchain-nightly.toml` is not used, and the fuzz job installs nightly explicitly. Rationale: reproducible builds.
4. `deny.toml`. `cargo-deny`: allowed licenses, banned crates, allowed-crate list, advisories. Rationale: D14.
5. `clippy.toml` and `.cargo/config.toml`. Pedantic Clippy; `-D warnings`; `unwrap_used` denied in library crates; `target-feature=+crt-static` for musl. The musl C and C++ toolchain for DuckDB's bundled build comes from CI environment variables, not from `.cargo/config.toml` (P0). Rationale: D14 and D24.
5a. `crates/tm-cli/src/main.rs` runs the real entry point on a spawned thread with an 8 MiB stack (`std::thread::Builder::stack_size`), since musl's default main-thread stack is too small for DuckDB's planner (NFR Portability). Listed here because it is a build-portability rule; the file is item 83's.
6. `.github/workflows/ci.yml`. Jobs: format and Clippy; `cargo test --workspace`; conformance suites; Python formats and checker cross-checks; differential; `cargo deny check` and `cargo audit`; `scripts/ci/check-owner-cap.sh`; musl release build of `tm`; `scripts/ci/symbol-versions.sh`; smoke containers; `scripts/ci/bench-index.sh`; coverage with `cargo-llvm-cov` against the 85 percent line; a nightly-toolchain fuzz job (`cargo-fuzz`, 600 s per target) on a schedule; an `insta` job with `cargo-insta`; and a privileged job that mounts an NFS loopback export for `tests/lock.rs`'s NFS case; the glibc 2.17 fallback build; `scripts/ci/spec-drift.sh`; the error-codes test. Rationale: every gate A exit criterion a machine can check.
7. `.github/workflows/release.yml`. Builds the static musl `tm` binary and the glibc 2.17 fallback; attaches checksums. `tm-agent` and `tm-bridge` are added by gate B. Rationale: G12.
8. `scripts/ci/check-owner-cap.sh`, `scripts/ci/smoke.sh`, `scripts/ci/symbol-versions.sh`, `scripts/ci/bench-index.sh`, `scripts/ci/spec-drift.sh`. `check-owner-cap.sh` greps every crate except `tm-owner-cap` and `tm-cli` for `new_for_cli` and builds every binary crate except `tm-cli` with the `construct` feature off (in gate A that set is empty and the script passes). `smoke.sh` runs the Verification Plan's smoke list. Rationale: the CI checks as scripts the owner can run.
9. `spike/musl-duckdb/` (`Cargo.toml`, `src/main.rs`, `README.md`). A throwaway binary linking `duckdb-rs` statically under musl with mimalloc and an 8 MiB main thread stack, running a fixed query with `ln`, `exp`, and `avg`, printing results and timing. Deleted in the change that lands item 10, after it has passed on CI (P0). Rationale: D24's first task.
10. `docs/decisions/musl-spike.md`. The spike's outcome, numbers, and the chosen build path. `docs/design/toulmin_v1_design.md` (Modify; this document, committed at that path) gains the pointer in its D24 row. Rationale: the build path decision must be recorded before P1.

#### 1. `crates/tm-formats/` (shared formats, pure)

11. `src/canonical.rs`. RFC 8785 canonical JSON serializer over `serde_json::Value`, refusing floats with `E1001`. Rationale: every ID depends on it (11.1, 11.2).
12. `src/id.rs`. `EventId`, `FileHandle`, `VersionHandle`, `ClaimRef`, `ReportHandle` newtypes; `derive_id(canonical bytes) -> EventId`; genesis string per chain. Rationale: 11.2.
13. `src/envelope.rs`. `Envelope`, `Actor`, `Source`; validation of `env`, `tv`, `chain`, `seq`, `prev`; the line form with `id` appended (11.2). Rationale: 11.2.
14. `src/events/mod.rs`, `project.rs`, `settings.rs`, `files.rs`, `reports.rs`, `claims.rs`, `audit.rs`, `chain.rs`, `lock.rs`. Typed bodies for exactly the 11.16 gate A types, each with its `tv`; a closed enum with an opaque catch-all variant that cannot be written. The reserved namespaces in 11.16 are not declared in gate A. Rationale: 11.16; D14.
15. `src/typed_value.rs`. The 11.6 encoding over `bigdecimal` at 34-digit precision, half-even; NFC for strings; conversion to and from Arrow scalars per the spec-type-to-Arrow mapping in item 47. Rationale: 11.6.
16. `src/parse_spec.rs`. The 11.7 schema, validation (`E3103` unknown column against a header, `E3104` encoding), and fingerprint. Rationale: 11.7.
17. `src/steps.rs` and `src/expr.rs`. The 11.8 operation types, each versioned; required-choice validation (`E6002`); the 11.18.1 grammar as an AST with a hand-written recursive-descent parser (`E6006`) and the 11.18.4 `{expr}` serializer over it (decision 7 of `decisions-p1.md`); column and type checking per 11.8 and 11.18.2 (`E6005`, `E6004`). Rationale: G2; 11.18.
18. `src/assertion.rs`. The 11.9 schema and the 11.18.3 rules as pure functions on typed values. Rationale: 11.9.
19. `src/citation.rs`. The 11.23 objects as an enum (`Cells`, `WholeFile`, `Page`, `Panel`, and the reserved kinds as opaque), their JSON encoding, and the parser for the 11.23 CLI spelling with its escapes. Rationale: 11.23.
20. `src/errors.rs`. The error catalog as an enum with stable codes, `rule`, `field`, `owner_command`, `subject`, and `gate` (`A`, `B`, `C`, or `D`: the earliest gate at which the code can be produced, a property of the catalog entry and not of an error value); the formats codes are the four listed in 11.12 (`E1001` to `E1004`); the full catalog is the union of these and every code in sections 9 and 11; when `TM_ERROR_LOG` is set, the constructor appends the code to that file. Rationale: 11.12.
21. `src/version.rs`. Format version constants and the reader policy. Rationale: 11.11.
22. `src/sort_order.rs`. Binary NFC comparison. Rationale: 11.10.
23. `tests/conformance.rs`. Every case under `conformance/formats/`; every `steps/invalid/` case rejected with the named code and field. Rationale: G2, G14.
24. `tests/rfc8785.rs`. The RFC's examples. Rationale: 11.1.
25. `build.rs`. Generator for `docs/errors.md` from the catalog, one row per code with its gate as a column; `spec-drift.sh` fails if the committed file differs. Rationale: 11.12.

#### 2. `crates/tm-fsfacts/` (filesystem facts, pure)

26. `src/snapshot.rs`. `StatSnapshot` (size, mtime, ctime, inode, no device) via `rustix` `statx` with `STATX_FORCE_SYNC` where available, open-before-stat fallback. Rationale: D19, D25.
27. `src/hash.rs`. Streaming SHA-256 with the `sha256:` prefix; thread count and bandwidth cap; timing. Rationale: D19.
28. `src/manifest.rs`. Directory manifests: entries sorted in 11.10 order, per-entry snapshot and fingerprint, manifest fingerprint, diff naming added, removed, and modified entries; exclude patterns and the `manifest.ignore` default. Rationale: D19.
29. `src/racy.rs`. The racy rule: a file whose mtime is at or after the mtime of the marker file the snapshot writes to the same filesystem is always re-hashed; when `hashing.trust_ctime` is false, every stat-changed file is re-hashed. Rationale: D19; Risks.
30. `src/needs_rehash.rs`. Pure decision from previous and current snapshots plus settings: `Unchanged`, `HashNow`, `Unconfirmed`. Rationale: D19; 3.4; 5.6 `possibly changed (unconfirmed)`.
30a. `src/probes.rs`. The `tm doctor` and `project.init` probes: filesystem type by `statfs` magic (network when in the NFS, Lustre, GPFS, CIFS, or FUSE set), mount options, exclusive-create and two-process rename-visibility probes on a given path, directory locality and writability. Rationale: section 9 `doctor` row; 3.1; `E2003`.
31. `tests/fsfacts.rs`. Temp-dir cases: unchanged; size change; mtime-unchanged content change caught by ctime; mtime-equal-to-marker re-hash; excluded entry; manifest diff with one added, one removed, one modified. Rationale: D19.

#### 2a. `crates/tm-duckdb/` (DuckDB engine, shared)

26a. `src/lib.rs`. `open(settings: EngineSettings) -> Engine`; `Engine::register_arrow(name, batch)`, `Engine::query(sql) -> RecordBatch`, `Engine::facts() -> {version, extensions}`; settings applied on open: `threads=1`, `memory_limit`, `temp_directory`, `autoinstall_known_extensions=false`, `autoload_known_extensions=false`, each set both in the `Config` the connection is created from and by a `SET` statement afterwards, because DuckDB's bundled build defaults both to on (P0); a per-call `interrupt` timer from `checker.call_seconds`; `facts()` also reports the build flavour (`musl` or `glibc`) for the verdict's `versions` block; the `duckdb` dependency lives only here. Rationale: 4.1; rule 1 (libraries never call each other); D24.

#### 2b. `crates/tm-index/` (index transaction, shared)

26b. `src/lib.rs`. `IndexTx` over `rusqlite::Transaction` with `execute(sql, params)`, `query_rows(sql, params) -> Vec<Row>`, and `head(chain) -> Head`; the `Param` and `Row` types the folds pass and receive; the `rusqlite` dependency lives only here. Rationale: rule 6 gives every library a fold signature naming this type, and no library may depend on the Core (rule 1).

#### 3. `crates/tm-record/` (Record library)

32. `src/layout.rs`. Creation and opening of the record directory: `record.json`, `main/`, `objects/sha256/`, `objects/tmp/`. `journals/` and `inbox/` are created by gates B and C. Rationale: 11.3; 11.21; glossary, Record directory.
33. `src/segment.rs`. Framed-batch append, segment roll, seal and pack, read from plain and packed segments, torn-tail detection, all per 11.21. Rationale: D9, 11.3, 11.21.
34. `src/head.rs`. `HEAD` read and write per 11.21, including the lock-time rewrite. Rationale: 11.3.
35. `src/lock.rs`. `O_EXCL` lock file, in-place heartbeat rewrite every `lock.heartbeat_seconds`, epoch increment written to `HEAD` first (the lock-time `HEAD` rewrite of 11.21), the pre-append fencing re-read (epoch and `HEAD.offset` against segment length), stale-heartbeat detection on the monotonic clock, `break`; on acquisition, torn-tail detection beyond `HEAD.offset`, truncation, and the queued `chain.torn_tail_truncated` event that item 75 puts first in the command's batch. Rationale: D15; 3.1.
36. `src/objects.rs`. Content-addressed kept-file store per 11.21. Rationale: D4.
37. `src/scan.rs`. Event scans: all, by type, after a position, by event ID (via the index's `events` table when present, else a linear scan). Rationale: section 9's resolve-by-ID rule.
38. `src/verify.rs`. Torn tail, fork, corruption, and object verification, reported separately as a `VerifyReport` enum with one variant per tampering case in item 41. Rationale: G13.
39. `src/append.rs`. The main-chain append entry point; `&OwnerCap` and a framed batch as arguments. Rationale: rule 4.
40. `tests/record_conformance.rs` and `examples/gen-chain.rs`. The test runs the cases under `conformance/record/`; the example generates a one-million-event chain into a directory for `bench-index.sh` and item 82. Rationale: G13, G14.
41. `conformance/record/` (listed here with its consumer): `chains/<name>/` (segments, `HEAD`, `expected.json`), `tamper/<case>/` for exactly these cases, each with the expected report name: `kept_byte_changed`, `segment_line_removed`, `segment_byte_changed`, `batch_hash_mismatch`, `torn_tail_beyond_head`, `fork_prev_mismatch`, `head_offset_mismatch`, `packed_segment_corrupt`, `verdict_data_changed` (the last is asserted at the Core level in item 82). Rationale: G13's listed cases plus 3.1.
42. `tests/lock.rs`. Two processes contend; a paused holder is fenced out after a simulated takeover; the same two cases on the CI NFS loopback mount. Rationale: D15; 3.1.

#### 4. `crates/tm-owner-cap/`

43. `src/lib.rs`. `pub struct OwnerCap(());` with no `Clone`, `Copy`, `Debug`, or `Default`; `pub fn new_for_cli() -> OwnerCap` behind `#[cfg(feature = "construct")]`; Core operations and the Record append take `&OwnerCap`. Rationale: D2; rule 4.

#### 5. `crates/tm-files/` (Files library, inputs in gate A)

44. `src/identity.rs`. Input identities (project path) with rename; the output identity type (analysis, pattern, relative path) declared for gate C. Rationale: hypothesis decision 7 as restated in 6.1: an input's identity is its repository-relative path.
45. `src/version.rs`. Versions with fingerprint, kept state, origin, content label; "new version if different content" with the three equivalences. Rationale: D5, D22.
46. `src/attributes.rs`. The 6.1 to 6.3 state machine as an enum with allowed transitions; `E3201`. Rationale: section 6.
47. `src/parse.rs`. CSV and TSV parsing with DuckDB's `read_csv` with every guess off (`header`, `delim`, `nullstr`, `columns` given explicitly, `all_varchar=true` then cast per spec); Parquet with `parquet`'s Arrow reader; decimal columns get their scale from a first pass over the text (the largest scale seen, recorded in the spec on `table.spec`); spec-type-to-Arrow mapping: `int` to `Decimal128(38,0)` (an integer wider than 38 digits is `E3102`), `dec` to `Decimal128(38,s)`, `str` to `Utf8`, `bool` to `Boolean`, `date` to `Date32`, `datetime` to `Timestamp(Nanosecond, UTC)`, `float64` to `Float64`, `missing` to null; key uniqueness check; `E3101`, `E3102`. One DuckDB connection per parse, opened through `tm-duckdb` (item 26a). Rationale: 11.7; M3 in the brief.
48. `src/parse_cache.rs`. Parquet cache written with the `parquet` Arrow writer, keyed by file fingerprint and spec fingerprint, under the cache directory. Rationale: brief, adopt as written.
49. `src/pdf.rs` and `src/image.rs`. Page count via `lopdf` reading the page tree only; panel names stored on the image version. Rationale: section 7 citation types.
50. `src/checksum.rs`. Provider checksum parsing: GNU `md5sum`/`sha256sum` two-space form and BSD `MD5 (file) = hash` form; one-time compare. Rationale: hypothesis decision 9 as restated in section 9's `file.register` row.
51. `src/fold.rs`. Files events to the 11.17 tables `files`, `file_versions`, `parse_specs`, `panels`. Rationale: rule 6.
52. `tests/files.rs`. On `fixtures/` (item 82a): registration; spec with each type; duplicate key; each equivalence rule; each allowed and each refused transition of 6.3; both checksum formats. Rationale: section 6.

#### 6. `crates/tm-checker/` (Checker library)

53. `src/translate.rs` and `src/eval.rs`. `eval.rs` evaluates `filter.where` and `derive.expr` over the Arrow batch with `bigdecimal` per 11.18.2 (34 significant digits, half-even); `translate.rs` turns the relational steps into explicit DuckDB SQL: one CTE per step, `NULLS FIRST|LAST`, key tie-breaker, `COLLATE BINARY`, `isnan` guards, `ties` mapping, group ordering, `member_count`, `member_keys_hash`. Rationale: D8; 11.18.
54. `src/engine.rs`. The Checker's use of `tm-duckdb` (item 26a): one connection per check, tables registered by name, version and extension facts captured into the verdict. Rationale: D24.
55. `src/run.rs`. Step-prefix execution: `eval.rs` for expressions, DuckDB for relational steps; per-step outputs and counts; `key_map_hash` and `final_rows_hash` per 11.18.2; assertion evaluation per 11.18.3. Rationale: 4.4.
56. `src/describe.rs`. The 11.18.4 templates. Rationale: the claim's writer never writes the step descriptions (11.18.4).
57. `src/rules.rs`. Required choices, `take` needs a total order, closed grammar; rule version constant. Rationale: D12.
58. `src/verdict.rs`. `Verdict` per the `claim.checked` body in 11.16. Rationale: section 7.
59. `tests/conformance.rs` and `examples/refcheck.rs`. The test runs `conformance/checker/cases/*/`; the example binary takes a case directory and writes `observed.json` in the same shape as `refcheck.py`'s output, never `expected.json`, so the differential script and the P5 checkpoint run without `tm`. Rationale: G14; D21.
60. `tests/property.rs`. `proptest` generators for tables, steps, and assertions; properties, 1,000 cases each: row permutation invariance; filter-then-sort equals sort-then-filter when the filter does not reference `rank`; `take n` is a prefix of `take n+1`; `rank` with `ties: ordinal` is a bijection; adding an unrelated column changes nothing. Rationale: G14.
61. `tests/snapshot_sql.rs`. `insta` snapshots of generated SQL for every conformance case. Rationale: a translator change is visible in review.
62. `fuzz/` (`Cargo.toml`, `fuzz_targets/steps_json.rs`, `fuzz_targets/derive_expr.rs`, `fuzz_targets/csv_parse.rs`). Rationale: hypothesis decision 30.

#### 7. `crates/tm-claims/` (Claims library)

63. `src/claim.rs`. Claim fields, kinds, refs, revisions, the current-revision rule for owner claims (the cross-journal rule of D29 arrives in gate B). Rationale: section 7.
64. `src/refs.rs`. `C<n>` allocation: the Core reads the highest `C<n>` from the index's `claims` table and confirms it against the chain's last `claim.added` before allocating under the lock; `S<n>.<m>` parsing arrives in gate B. Rationale: D16.
65. `src/evidence_exists.rs`. The 7.3 table as an exhaustive match. Rationale: G1.
66. `src/flags.rs`. Flag and label enums from 5.6. Gate A raises exactly these on claims, spans, and files: event-derived and stored in `flags` by the Claims fold: `assertion fails`, `promoted, not reviewed`, `evidence not kept`, `removable`, `restored`, `retracted`, `superseded`; scan-derived and computed at read time by `read/flags.rs` from the stat cache, the scanned `spans` table, and `report.check`: `text drifted`, `evidence moved` (disk differs), `expected-missing`, `possibly changed (unconfirmed)`, `points at a retracted claim`, `points at a superseded claim`. The last two are scan-derived because each needs a scanned span and a chain state together. `retired` arrives with gate C. Owner-action labels (`possibly agent-issued`, D23) are item 72's. Rationale: 5.6; 11.17 fold contract.
67. `src/reviews.rs`. Review events and the rule that reviews never change verdicts. Rationale: section 7.
68. `src/spans.rs`. The 11.22 scanner. Rationale: D10; 11.22.
69. `src/drift.rs`. Comparison of the recorded sentence with the normalized span text per 11.22. Rationale: G11.
70. `src/fold.rs`. Claims events to the 11.17 tables `claims`, `claim_revisions`, `claim_cites`, `claim_keys`, `claim_events`, `verdicts`, `reviews`, `flags` (event-derived rows only, per item 66), `reports`. The fold never writes `spans`, which is scan-derived (11.17). Rationale: rule 6.
71. `tests/claims.rs` and `tests/spans.rs`. Every cell of the 7.3 table reachable in gate A; span fixtures: plain, escaped bracket, two attributes, duplicate ref, span in fenced code (ignored), span with inline code (not checkable), NFC-different but equal text, whitespace-run-different but equal text, heading class capture. Rationale: G1, G11.

#### 8. `crates/tm-core/` (Core)

72. `src/actor.rs`. `Actor::Owner(&OwnerCap)` and `Source` detection: `isatty` on standard input and the `harness.env_vars` list (default `AGENTBOX_SESSION` and `CLAUDECODE`; the label records `AGENTBOX_AGENT`'s value when set). Rationale: D23.
73. `src/project.rs`. Per-command resolution: read `.toulmin/project.json`, read the binding under `~/.config/toulmin/projects/`, open `record.json`, cross-check IDs (`E2004`, `E2005`). Rationale: OQ6.
74. `src/ops/mod.rs` and one file per gate A operation: `project_init.rs`, `settings_set.rs` (also `show` and `unset`), `read_in.rs` (gate A: scans nothing; the entry point exists so every locked command has one path), `file_register.rs`, `file_hash.rs` (the explicit hash step behind `--defer-hash`), `table_spec.rs`, `file_attrs.rs`, `file_panels.rs`, `claim_add.rs`, `claim_check.rs`, `claim_review.rs`, `claim_update_text.rs`, `claim_retract.rs`, `claim_supersede.rs`, `report_add.rs`, `report_check.rs` (both rescan every registered report with item 68's scanner and rewrite the `spans` table), `view_export.rs`, `apply.rs` (gate A: `claim.review` items only; `E8102` for others), `audit.rs`, `lock.rs`, `doctor.rs`, `status.rs` (also rescans spans, per 11.17). Rationale: section 9, one file per row.
75. `src/batch.rs`. Batch assembly per 11.21: a queued `chain.torn_tail_truncated` first, then the command's events; `seq`, `prev`, `batch`, `epoch`, IDs; hand-off to `tm-record::append` with the `&OwnerCap`. Rationale: all-or-nothing writes.
76. `src/index/mod.rs` (opens the database and hands `tm-index`'s `IndexTx` to each fold; it does not define the type), `src/index/schema.sql` (the 11.17 text), `src/index/apply.rs` (runs the folds and writes the Core-owned tables `head`, `events`, `opaque_events`, `settings`, `audits`, `claim_events`), `src/index/locate.rs` (path resolution order of 3.4, `PRAGMA integrity_check`, rebuild trigger). Rationale: 11.17; 3.4.
77. `src/stat_cache.rs`. The node-local stat cache of 3.4: the `stat_cache` table defined in 11.17, excluded from rebuilds; `tm status` writes `file.version_added` with origin `changed_outside_run` for a registered input whose confirmed fingerprint differs from disk (6.2, CUJ 7). Rationale: 3.4.
78. `src/read/mod.rs` and one file per gate A query family: `files.rs` (including `files.lineage`), `claims.rs`, `reports.rs`, `flags.rs` (scan-derived flags per item 66), `settings.rs`, `audit.rs`, `checker.rs`. Typed parameters and results; every function has a `--json` rendering. Rationale: G9.
79. `src/view/mod.rs`, `src/view/pages.rs`, `src/view/templates/{index,report,claim,file,flags,audit}.html`, `src/view/static/style.css`, `src/view/static/actions.js`, `src/view/actions.rs`. Per 11.20. Rationale: D18; 11.20.
80. `src/audit.rs`. `tm audit` orchestration: verify, full re-hash, re-run every claim check, compare verdicts, list open items, write `audit.json` (`{"audit":1,"at","since","files":[{"path","version","expected","observed","last_confirmed","dependents"}],"verdicts":[{"ref","revision","stored","observed","cause"}],"open":{"flags":[...],"unreviewed":[...]}}`) and `audit.html`; `--since LAST` (`LAST` is the literal word, meaning the latest `audit.completed`, or an `audit.completed` event ID) skips re-hashing files whose stat cache entry was confirmed at or after that audit. Rationale: G13.
81. `src/settings.rs`. The 11.19 table as a schema with validation. Rationale: 11.19.
82. `tests/scenarios/main.rs` with `mod` declarations for `register_and_claim.rs`, `human_writer.rs` (CUJ 6), `correct_claim.rs` (CUJ 5 a to c, owner claims only), `audit_tampering.rs` (every item 41 case plus `verdict_data_changed`, each asserting the report name), `audit_idempotent.rs` (G13: audit twice on the untouched fixture; `counts.mismatches` and `verdicts_changed` are 0 and the two `audit.json` files differ only in `at`), `static_view_parity.rs` (G9: CLI `--json` equals each page's `tm-data` block), `holds_scan.rs` (G1: no page shows holds for a non-holds verdict), `claim_pages.rs` (G3: each of the ten claim pages contains the cited-rows section, one description per step, and one tree node labeled with the registered input), `static_view_apply.rs` (review round trip), `index_rebuild_budget.rs` (asserts the rebuild on the chain from `gen-chain.rs` completes; the time budget itself is `bench-index.sh`). All scenario tests use the workspace `fixtures/` of item 82a. Rationale: CUJ 5, 6, 8 as executable tests; the gate A exit criteria.

82a. `fixtures/` at the workspace root (created in P3, used by items 52, 82, 86, and the smoke test). Tables: `de.tsv` (gene_id, log2FoldChange, padj, baseMean), `samples.tsv` (sample_id, condition, batch), `counts.tsv` (gene_id plus six sample columns); `specs/*.json`; `figure.pdf` (two pages) and `figure.png` with panel names `A` and `B`; `report.md` with ten spans; `claims/<n>/` each holding `args.txt` (the `tm claim add` arguments), `steps.json`, and `assert.json`; `expected.json` with each claim's verdict and flags. The ten claims cover every 11.18 op and every assertion kind: 1 `filter`+`sort`+`take`, `rank`; 2 `filter`, `value`; 3 `group` with `mean` and `count`, `value`; 4 `join`+`derive`, `compare_rows`; 5 `rank` with `scope: group`, `exists`; 6 `select`+`filter`, `count`; 7 qualitative, `whole_file`; 8 qualitative, `page`; 9 qualitative, `panel`; 10 mixed, `cells` with a composite key and `value`. `ACCEPTANCE.md`: the owner's checklist for the manual exit criterion (paper named, tables registered, every claim in the paper's results section entered, every verdict reviewed in the static view), signed by date in the file. Rationale: one fixture for every suite.

#### 9. `crates/tm-cli/` (binary `tm`)

83. `src/main.rs`. `clap` tree for the gate A objects and verbs: `init --record PATH | --attach PATH`; `status [--refresh] [--hash] [--flags]`; `file register PATH --title T --description D [--source S] [--checksums FILE] [--defer-hash]`, `file hash HANDLE [--again]`, `file show|versions|rows|list|lineage [--depth N]|keep [--fingerprint-only]|removable|restore|rename FROM TO|panels HANDLE NAME...`; `table spec PATH --key COL[,COL] --types COL:TYPE,... [--missing [COL:]TOKENS]... [--nan missing|value] [--inf missing|value] [--no-header] [--delimiter C] [--format csv|tsv|parquet]`, `table preview HANDLE [--steps FILE]` (`files.rows`, or `checker.preview_steps` with `--steps`), `table describe --steps FILE` (`checker.describe_steps`); `claim add --report PATH --ref REF --sentence S --cited HANDLE... [--steps FILE] [--assert FILE] --kind quantitative|qualitative|mixed [--qualifiers Q]`, `claim show|list|evidence|history|citing-rows|check REF`, `claim review REF --outcome confirmed|disputed|question [--note N]`, `claim update-text REF`, `claim retract REF --reason R`, `claim supersede REF` (with `claim add`'s arguments for the replacement); `report add|spans|check`; `view --out DIR`; `apply FILE [--yes-override]`; `audit [--since LAST] [--out DIR]`, `audit last`; `doctor`; `lock status|break`; `settings show|set KEY VALUE...|unset KEY` (list-valued keys take repeated `VALUE` arguments and `set` replaces the whole list). Every command accepts `--json`. Exit codes: 0 success, 1 refused by a catalog error (code printed), 2 usage, 3 lock not acquired, 4 integrity failure found. One `OwnerCap` construction per process. Rationale: section 12; G9.
84. `src/commands/{init,status,file,table,claim,report,view,apply,audit,doctor,lock,settings}.rs`. Rationale: one file per object.
85. `src/doctor_report.rs`. Renderer for the Core's `doctor` result (item 74's `doctor.rs` runs item 30a's probes on the record path and the cache directory, and adds glibc and kernel versions and the `tm-duckdb` facts). Rationale: section 9 `doctor` row.
86. `tests/cli.rs` and `tests/error_codes.rs`. End-to-end over `fixtures/` with `assert_cmd`, one test per command in item 83 that has a reachable gate A path (`file restore` is exercised only for its `E3201` refusal) and one per exit code; `error_codes.rs` is the Verification Plan's error-code diff. Rationale: G12.

#### 10. Python reference implementations

87. `reference/checker/README.md`, `refcheck.py`, `typed_values.py`, `ops.py`, `assertion.py`, `describe.py`, `validate.py` (step and assertion schema validation emitting code and field, for G2). Plain Python 3, standard library plus `reference/formats/canonical_json.py` (needed for the three hashes in 11.18.2), written by a separate agent session from `docs/spec/checker.md` and the conformance cases' inputs only. `expected.json` files are written by a third agent session that reads `docs/spec/checker.md` and the case inputs only, at the start of P4, before either checker exists, and are checked by the owner at the P4 sign-off. Neither implementation ever writes them, which is what makes a conformance failure informative. When the two checkers differ on a case, the difference is recorded in `docs/decisions/checker-diffs.md` and ruled against `checker.md`; the implementation that disagrees with the spec is fixed, and the spec is amended only through that record. Rationale: D21.
88. `reference/formats/canonical_json.py`, `ids.py`, `test_formats.py` (standard library `unittest`, run as `python3 reference/formats/test_formats.py`). Rationale: 11.1 cross-check.
89. `scripts/differential.sh` and `crates/tm-checker/examples/gencases.rs`. The Rust generator (owned by the Rust side, not the Python session) writes N cases covering every op, every type, missing and NaN values, ties, composite keys, and both join kinds; the script runs `examples/refcheck.rs` and `refcheck.py` on each, compares every field including `final_rows_hash` (11.6's 15-significant-digit rounding removes the libm difference, so no tolerance file exists), and reports differences as a table. The CI job runs it on two named runner classes of different CPU generations. Rationale: G14; 11.18.2.

#### 11. Conformance cases and specifications

90. `conformance/formats/`: `canonical/*.json`, `ids/*.json`, `envelope/*.json` (one valid and one invalid line per 11.16 type, with every 11.23 citation kind appearing in at least one `claim.added` case), `typed_values/*.json`, `steps/valid/*.json`, `steps/invalid/*.json` (each names the expected code and field), `expr/*.json` (grammar cases: precedence, escapes, backtick columns, each error), `assertions/*.json`. Rationale: G2, G14.
91. `conformance/record/`: the directory of item 41.
92. `conformance/checker/cases/<name>/` with `tables/*.csv`, `specs/*.json`, `steps.json`, `assertion.json`, `expected.json` (per-step counts, final rows, descriptions, verdict). At least 60 hand-checked cases: each op; each tie rule; each missing placement; NaN and Inf; composite keys; each join kind; group with each aggregate; each assertion kind and operator; each 11.18.2 domain rule. Rationale: the anchor for the Python checker.
93. `docs/spec/formats.md` (sections 11.1 to 11.17, 11.21, and 11.23), `docs/spec/record.md` (section 3 and 11.21), `docs/spec/checker.md` (11.6 to 11.10 and 11.18, with the conformance case format), `docs/spec/claims.md` (section 7 and 11.22), and `docs/decisions/checker-diffs.md` (one entry per case where the two checkers disagreed, with the ruling against `checker.md`). The four specs are normative text copied from the design and kept in step by `scripts/ci/spec-drift.sh`. Rationale: D21 for `checker.md`; G14 for the rest.
94. `docs/errors.md`, generated by item 25. Rationale: 11.12.
95. `docs/user/getting-started.md` (init, register, spec, claim add, spans, view, audit, each with the fixture as the worked example), `docs/user/claims.md` (citation syntax, steps file, assertion file, reading a claim page). `docs/user/clusters.md` arrives in gate C. Rationale: the exit criterion's procedure.
96. `README.md`. What Toulmin is, the threat stance in three sentences, install by copying the binary, link to the docs. Rationale: distribution.
97. `LICENSE` (permissive, code) and `docs/spec/LICENSE` (public domain dedication). Rationale: Summary decision kept.

### Verification Plan

**Build.** `cargo build --release --target x86_64-unknown-linux-musl -p tm-cli`; the glibc 2.17 fallback with `cargo zigbuild --target x86_64-unknown-linux-gnu.2.17` and `-g0` (a CI job, not only release); `scripts/ci/symbol-versions.sh` reports no glibc symbol version above 2.17 in the fallback binary and no dynamic dependency in the musl binary.

**Static analysis.** `cargo fmt --check`; `cargo clippy --all-targets -- -D warnings` at pedantic level with `unwrap_used` denied in library crates; `cargo deny check`; `cargo audit`; `scripts/ci/check-owner-cap.sh`.

**Test suites.**

| Suite | What it checks | Command | Pass condition |
| --- | --- | --- | --- |
| Conformance: formats | Canonical JSON, IDs, envelopes per type, typed values, steps and expression validation, assertions | `cargo test -p tm-formats --test conformance`; `python3 reference/formats/test_formats.py` | Every case passes in both implementations with identical hashes; every `steps/invalid/` case is rejected with the named code and field by both |
| Conformance: record | Chains, packing, every item 41 tampering case | `cargo test -p tm-record --test record_conformance` | Each tampering case reported by its name; untouched chains verify clean |
| Conformance: checker | 60 or more hand-checked cases | `cargo test -p tm-checker --test conformance`; `python3 reference/checker/refcheck.py conformance/checker` | Identical per-step counts, final rows, descriptions, and verdicts in both |
| Property and metamorphic | The five item 60 properties | `cargo test -p tm-checker --test property` | 1,000 cases per property, no failures |
| Differential | Rust Checker against the Python reference | `scripts/differential.sh 10000` | No difference in any field, including `final_rows_hash`. The six IEEE-double functions are reconciled by the 15-significant-digit rounding in 11.6, not by a comparison tolerance, so there is no tolerance file and any difference is a defect |
| Snapshot | Generated SQL per conformance case | `cargo insta test -p tm-checker` | `cargo insta` reports no pending snapshots |
| Fuzzing | Steps JSON, expression grammar, CSV parsing | from `crates/tm-checker/`: `cargo +nightly fuzz run steps_json`, `derive_expr`, and `csv_parse`, each with `-- -max_total_time=600`, scheduled job | No crash, hang, or panic in 600 s per target |
| Unit | Unit tests in every crate | `cargo test --workspace` | All tests pass |
| Scenario | The eight item 82 tests | `cargo test -p tm-core --test scenarios` | All tests pass |
| CLI | One test per command and exit code | `cargo test -p tm-cli` | All tests pass |
| Index budget | Full rebuild on the one-million-event chain from `examples/gen-chain.rs` | `scripts/ci/bench-index.sh` | Under 60 s on the CI runner class named in the script |
| Lock on NFS | Item 42's two cases on the loopback export | `cargo test -p tm-record --test lock -- --ignored nfs` in the privileged job | Both cases pass |
| Smoke | Both release binaries run on old distributions | `scripts/ci/smoke.sh` in CentOS 7, Rocky 8, and Rocky 9, once with the musl binary and once with the glibc fallback | `tm init`, `tm file register`, `tm table spec`, `tm claim add`, `tm view --out`, and `tm audit` exit 0 on the fixture |
| Coverage | Line coverage | `cargo llvm-cov -p tm-formats -p tm-record -p tm-checker -p tm-claims --fail-under-lines 85` | Passes |
| Error codes | Every gate A `E` code is produced by a test | `cargo test -p tm-cli --test error_codes` after the other suites, with `TM_ERROR_LOG` set | The catalog's error constructor appends each code to `$TM_ERROR_LOG` when the variable is set; the test diffs that file against the catalog entries whose `gate` is `A` and fails on any such code never produced |
| Spec drift | `docs/spec/*.md` and `docs/errors.md` match their sources | `scripts/ci/spec-drift.sh` | The four spec files equal the named sections of `docs/design/toulmin_v1_design.md` after heading normalization (on both sides the `#` run and any section number are dropped from heading lines, trailing whitespace is removed, and blank-line runs are collapsed, as `spec-drift.sh` states); `docs/errors.md` equals `build.rs` output |

**Manual criteria.** The musl spike decision is recorded (item 10); the owner has completed `fixtures/ACCEPTANCE.md` on one published paper. Both are listed in the gate A row of the gate exit criteria.

--------------------------------------------------------------------------------

<!-- Provenance: Indicates initial structural drafting via EGM. -->

*Drafted with EGM*
