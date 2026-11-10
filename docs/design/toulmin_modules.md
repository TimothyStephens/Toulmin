# Toulmin V1: modules and boundaries (version 3)

Nov 9, 2026 · replaces version 2 · read with `toulmin_hypothesis.md` (version 3)

## Layout

| Part | Kind | One line |
| --- | --- | --- |
| Shared formats | Shared code (pure) | Event envelope, IDs, canonical JSON, typed values, citations, query step operations, assertion schema, receipts, declarations, handshake messages, and error codes |
| Filesystem facts | Shared code (pure, no state) | Stat snapshots, hashing, directory manifests, exclusions, and the racy rule; takes paths and settings, returns facts |
| Record | Library | Main-chain segments, journals, inbox, lock and epochs, kept-files folder, generic event scans, and integrity checks |
| Files | Library | File identity, versions, attributes, parse specs and keys, and the parse cache |
| Runs | Library | Declarations, receipts, run checks, lineage per version, approval scope, and attribution matching |
| Checker | Library | Query steps to explicit DuckDB SQL, assertions, general rules, and result fingerprints |
| Claims | Library | Claims, refs, revisions, verdicts, reviews, flags, new-version re-checks, and report-span scanning and drift |
| Core | Application layer | Every user-level operation, all permission checks, staleness, the read interface, journal and receipt ingest, and index folds |
| CLI (`tm`) | Front end | Owner commands, including the run wrapper (as a Core operation) and `tm apply` |
| Agent tool server | Front end | One per session, outside the sandbox; MCP over a Unix socket, reached through the stdio bridge |
| Live viewer | Front end | Read interface, reviews, and owner actions through a confirmation step |
| Static view | Output of the Core | `tm view --out DIR`: pages plus action-file export |

The five libraries are synchronous. Async code exists only in the agent tool server and the live viewer.

## Rules between parts

1. **Libraries never call each other.** The Core passes data between them; for example, a parsed table from Files into the Checker, and Checker results into Claims.
2. **Shared formats and filesystem facts** can be used by any part. They hold no state and never touch the record.
3. **Front ends only call the Core.**
4. **Permissions are checked only in the Core.**
   - Libraries receive plain, already-authorized decisions.
   - Owner operations need an owner capability value. Its constructor lives in a crate only the CLI and the viewer's confirmation path depend on, and CI checks the agent tool server's dependency tree.
5. **Who writes what:**
   - Only the CLI writes the main chain, under the lock.
   - Tool servers and the viewer append only to their own journals.
   - The run wrapper writes only receipts.
6. **Index ownership.** Each library supplies its own fold from its events to index tables, and the Core runs the folds. The Record stores derived tables without knowing what they mean.

## Who owns which events

| Events | Owner | Written to |
| --- | --- | --- |
| Project created, settings changed, lock taken, journal head recorded, receipt ingested | Core | Main chain |
| Session started, ended, closed, abandoned; promote (with any override); send back; owner action via viewer or `tm apply` | Core | Main chain |
| File tracked, registered, version added (including "changed outside a run"), kept, marked removable, retired, restored, renamed; parse spec set; hashing finished; provider checksum checked | Files | Main chain |
| Analysis declared; run started and finished (from receipts); run checked; approval (review of a code version) | Runs | Main chain |
| Claim proposed or revised; title and description proposed; analysis declaration drafted; draft note | Claims or Runs (by subject) | Session journal |
| Claim added by owner, checked, reviewed, accepted onto a new version, text updated, retracted, superseded; report registered | Claims | Main chain (reviews from the viewer: viewer journal) |
| DuckDB or checker upgrade, with the changed-verdict report | Core | Main chain |

Computed, not stored: staleness, cited and uncited counts, agent attribution, and "other claims citing these rows".

## What crosses each boundary

### Shared formats and filesystem facts

- **Shared formats:** types and pure functions only, such as canonical JSON and ID derivation.
- **Filesystem facts:**
  - **In:** paths, exclude lists, settings, and previous stat values.
  - **Out:** snapshots, hashes, manifests, and "needs re-hash" results.

### Record

- **In (from the Core only):**
  - a framed batch for the main chain;
  - kept-file paths with their expected hashes (the Record streams, compresses, and checks them).
- **Journals** are appended by their own writers through a Record journal API.
- **Out:** event scans (all, by type, or after a position), journal reads, inbox listings, kept bytes, and integrity results (torn tail, fork, corruption).

### Files

- **In:** filesystem facts, declarations' expanded paths from the Core, receipts' output facts, and owner decisions passed in by the Core.
- **Out:**
  - identities, versions, and attributes;
  - Arrow tables from the parse cache;
  - page counts (PDF) and registered panel labels;
  - content-equivalence results (same bytes, same table by key, same decompressed content).

### Runs

- **In:** declarations, receipts, git facts (commit, script diff, commit list with patch IDs), session reports, and file version IDs from Files.
- **Out:**
  - run records and run-check results;
  - lineage edges between versions;
  - code-version identities for approval;
  - matched agent commits.

### Checker

- **In:** a named set of Arrow tables with their parse-spec fingerprints, the query steps, the assertion, and the rule version.
- **Out:**
  - per-step outputs and counts;
  - plain-language descriptions;
  - final rows mapped to input keys;
  - the assertion result;
  - the generated SQL;
  - the result fingerprint;
  - all version information.

### Claims

- **In:**
  - proposals;
  - Checker results;
  - Files lookups (key present, page count, panel labels, availability);
  - staleness, approval state, and version status per cited version, all from the Core;
  - report file text from the working tree;
  - reviews.
- **Out:** claims, refs, revisions, verdicts, reviews, flags with reasons and clearing actions, and drift results.

### Core

- **In:** requests from the front ends, each carrying an actor (owner, owner via viewer, or session).
- **Out:** results and typed errors.
- **Owns:**
  - the operations table (actor, permission, library calls in order, events, errors);
  - staleness;
  - the read interface;
  - ingest;
  - index folds.

### Agent tool server

- **In:** MCP calls through the bridge, authenticated by the session token.
- **Out:** Core requests made as that session, with results that carry flags.
- **Logging:** each call goes to the session's diagnostic log.
- **Tools:** query, preview steps, propose and revise claims, propose titles and descriptions, propose accepting a new version, propose supersede, draft an analysis declaration, and write draft notes.
- **Never:** owner actions, registering inputs, or running analyses.

### Live viewer and static view

- **In:** owner browser requests with the per-launch token (live viewer only).
- **Out:**
  - pages from the read interface;
  - the analysis tree per claim;
  - reviews and confirmed owner actions (live viewer);
  - an action file (static view).

## External boundaries

| External system | Direction | What crosses | Notes |
| --- | --- | --- | --- |
| git | Toulmin reads | Commits, script diffs, patch IDs, report files (working tree) | Never commits or changes branches |
| agentbox | Both ways | Versioned handshake: start (session number, token, bridge, lists) and end (clone path and commits) | Unisolated sessions use the same messages without agentbox |
| Agent harness | Both ways | MCP over stdio to the bridge, then the Unix socket | Agent-denied files are refused |
| Scheduler | Toulmin reads only | Environment variables matching `SLURM_`, `PBS_`, `SGE_`, `LSB_` | Never submits or queries jobs; catches `SIGTERM` |
| Workflow tools | Wrapped | Head-process command, exit status, and declared files | Per-rule lineage in V1.x |
| Filesystem | Both ways | Project tree (stat and hash), record directory (main chain, journals, inbox, kept files) | `tm doctor` checks reachability and mount options |
| Browser | Both ways | Static pages, live pages, reviews, and confirmed actions | Token, Host and Origin checks, sanitized Markdown, content security policy |
| DuckDB | Embedded | Explicit SQL, settings, version, and extensions | Pinned exactly; no auto-loading |

## Build gates

| Gate | Parts | Usable for |
| --- | --- | --- |
| A | Musl spike; shared formats and filesystem facts; Record; Core and CLI; Files (inputs); Checker with conformance, property, and differential tests and the Python reference; Claims and report spans; static view; `tm audit` | Hand-written claims on registered tables, such as a published paper's tables |
| B | Agent tool server, bridge, handshake (agentbox and unisolated), journals, refs, promote, send back, `tm apply` | Agents drafting claims and text on registered inputs |
| C | Runs (wrapper, receipts, inbox, lineage, run groups), staleness, new versions, approval and carry-over, attribution, drafted declarations | The full analysis tree |
| D | Live viewer | Interactive review |
