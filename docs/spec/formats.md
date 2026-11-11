<!-- design-sections: 11.1-11.17, 11.21, 11.23 -->
## 11.1 Canonical JSON

Canonical JSON is RFC 8785 (JCS), implemented in-house and tested against the RFC's examples and a second Python implementation. Hashed records hold no JSON floats: integers within plus or minus 2^53 are JSON integers; everything else is a string. Times are RFC 3339 strings in UTC with nanoseconds as a separate integer field where sub-second precision is recorded (snapshots and receipts). Non-UTF-8 paths are refused with `E3003`. Input to the canonicalizer must be I-JSON (RFC 7493): valid UTF-8, no duplicate member names, no lone surrogates, and no `NaN` or `Infinity`; anything else, including any JSON syntax error, is `E1003`. A number token with a fraction or an exponent, a minus zero (`-0`), or an integer outside plus or minus 2^53 (inclusive) counts as a float for this rule and is `E1001`. When an input has several faults, the one reported is the first a left-to-right read meets, except that a duplicate member name is reported when its object closes and a lone surrogate only after the whole input is read.

## 11.2 Event envelope

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

## 11.3 Chain link and batch framing (main chain)

- Segment files `main/seg-NNNNNN.jsonl`, sealed before a batch that would carry them past 64 MiB (a single larger batch is its own segment); sealed segments are packed with zstd to `seg-NNNNNN.jsonl.zst` and the original removed after the packed file is synced and verified.
- A batch is: one header line `{"b":<batch>,"n":<events>,"len":<payload bytes>,"h":"sha256:...","epoch":<e>}`, then exactly `len` bytes of newline-terminated events. `h` is the hash of the payload. A header whose `len` is not satisfied, or whose `h` does not match, is a torn tail.
- `main/HEAD` is a small JSON file rewritten by write-then-rename after every batch: `{"seq","segment","offset","last_id","batch","epoch","format":1}`.
- `main/lock` holds `{"host","pid","started","heartbeat","epoch"}`.
- Record header `record.json`: `{"format":1,"project_id","owner_id","created","hash_alg":"sha256"}`.

## 11.4 Journal format

`journals/S4.jsonl` and `journals/V2.jsonl`: one envelope per line, no batch framing, `prev` links lines. The main chain's `journal.head_recorded` body is `{"journal":"S4","seq":128,"id":"sha256:..."}`. Read-in accepts only the event types allowed for the journal's kind (session journals: `claim.proposed`, `claim.revised`, `title.proposed`, `declaration.drafted`, `supersede.proposed`, `accept.proposed`, `note.drafted`, `journal.torn_tail_truncated`; viewer journals: `claim.reviewed`, `run.reviewed`, `viewer.action_confirmed`); any other type is recorded on the main chain as `journal.event_quarantined` with the offending line's ID and is never interpreted.

## 11.5 Receipt format

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

## 11.6 Typed values

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

## 11.7 Parse spec

`{"spec":1,"format":"csv|tsv|parquet","delimiter","header":true,"columns":[{"name","type","missing":["NA",""]}],"key":["gene_id"],"nan":"missing|value","inf":"missing|value","encoding":"utf-8"}`. `type` is one of `int`, `dec`, `str`, `bool`, `date`, `datetime`, or `float64` (item 47's mapping); `missing` is the list of tokens read as `missing` in that column; a `dec` column also carries `scale`, a JSON integer from 0 to 38 that `table.spec` records as the largest scale seen (item 47), and no other type carries it. Every key shown is required: `delimiter` is a one-character string and `header` a boolean for `csv` and `tsv`, and both are `null` for `parquet`. Column names are non-empty and distinct; `key` is a non-empty list of distinct column names. A key name the columns list does not have, or a column the file's header does not have, is `E3103`; any other schema violation is `E3105`; `field` points at the offending member. `utf-8` is the only encoding accepted in V1; another value is refused with `E3104`. Its fingerprint is the ID of its canonical JSON. Key uniqueness is checked at `table.spec` and again when a new version arrives.

## 11.8 Query steps

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

## 11.9 Assertion schema

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

## 11.10 Text sort order

Binary order of NFC-normalized UTF-8 bytes, which is DuckDB's default binary collation. No locale collation anywhere. The SQL sets `ORDER BY ... COLLATE BINARY` explicitly.

## 11.11 Version policy

Every format carries its own version number: record header, envelope, each event type, receipts, parse spec, steps, each op, assertions, handshake, action file. Readers accept any version they know; an unknown event type or a higher version is read as opaque and never rewritten, and writing an event with an unknown version is refused. Index folds skip opaque events and record that they did.

## 11.12 Error catalog

Stable codes `E<library><nnn>`: 1 formats, 2 record, 3 files, 4 runs, 5 checker, 6 claims, 7 core and permissions, 8 front ends. Each error is `{"code","message","rule","field","owner_command","subject"}`; `rule` and `owner_command` are filled for agent refusals. `field` is a JSON Pointer (RFC 6901) into the document the error is about, for example `/ops/1/ties`, and is the same string in both the Rust and the reference implementation. Each catalog entry also carries a `gate` of `A`, `B`, `C`, or `D`, the earliest gate at which that code can be produced, so a gate's conformance test can filter the catalog to the codes it is allowed to expect. `field` is `null` when the input is not a JSON document (a CLI argument) and for `E1001` and `E1003`, which are about the input as JSON text. The formats codes are `E1001` float or out-of-range number in canonical JSON, `E1002` invalid envelope, `E1003` input not I-JSON, and `E1004` invalid typed value. An error inside an envelope line is reported as `E1002` with the pointer into the line, whatever code the nested object's own validation would give standing alone. Messages may change; codes never do.

## 11.13 Reserved fields

Envelope: `sig`, `collab`. Run body: `parent_run`. Citation kinds: `quote`, `page_region`, `image_region`. Verdict body: `model_check`. Session body: `isolation: separate-host`. Reserved fields are accepted and stored as given, never interpreted in V1.

## 11.14 Handshake schema

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

## 11.15 Action file (static view to `tm apply`)

`{"actions":1,"exported_at","chain_head":"sha256:...","items":[{"op":"draft.promote","targets":["S4.12@<revision id>"],"allow_unapproved":false},{"op":"run.approve","code_version":"..."},{"op":"claim.review","ref":"S4.12","revision":"sha256:...","outcome":"confirmed","note":"..."}]}`. `tm apply` refuses an item whose target changed since `chain_head` (`E8101`), prints one line per item with its target, revision, and any override, requires `--yes-override` for any item that sets an override, and asks for confirmation. Review items carry the revision they were made against.

## 11.16 Event bodies (gate A types)

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

## 11.17 Index schema

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

## 11.21 Main-chain write sequence and object store

**Lock-time `HEAD` write.** Taking or breaking the lock rewrites `HEAD` with the incremented epoch and every other field unchanged, by the same write-then-rename sequence as step 3 below.

**Batch write.** (0) If the open segment's length plus the batch would exceed 64 MiB, seal the open segment (see Sealing) and start a new one; a batch never spans two segments, so a single batch larger than 64 MiB is a segment of its own. (1) Serialize the events, each as one line ending in `\n`; `len` is the byte count of all event lines including their newlines; `h` is `sha256:` of exactly those bytes. (2) Write the header line (terminated by `\n`, not counted in `len`, not covered by `h`) and the event bytes to the open segment with one `write` call per line, then `fsync` the segment. (3) Write `HEAD` to `main/HEAD.tmp`, `fsync`, rename to `main/HEAD`, `fsync` the `main/` directory. `HEAD.offset` is the byte length of the open segment after step 2. `seq` increases by one per event across batches; `HEAD.seq` is the last event's `seq`. **Sealing** is done by the writer that would overflow: it compresses `seg-NNNNNN.jsonl` to `seg-NNNNNN.jsonl.zst` at zstd level 9, `fsync`s it, re-reads and verifies every batch in the packed copy, then removes the plain file. Readers accept either form.

**Objects.** Path `objects/sha256/<first two hex>/<remaining hex>.zst`, addressed by the fingerprint of the raw bytes. Keep: stream the source through SHA-256 and zstd (level 3) into `objects/tmp/<random>`, compare the computed fingerprint with the expected one (`E3203` on mismatch), `fsync`, rename into place, `fsync` the directory.

## 11.23 Citation objects

One JSON object per citation, stored in `claim.added` and `claim.revised` bodies, in `claim_revisions.citations`, and in the conformance envelopes. Key values use the 11.6 typed-value encoding typed per the cited table's parse spec. The CLI spelling in item 19 is parsed into exactly these objects.

| Kind | Keys | Notes |
| --- | --- | --- |
| `cells` | `{"kind":"cells","version","keys","columns"}` | `version` is a version handle; `keys` is a list of key tuples, each a list of typed values in the spec's key-column order; `columns` is the list of cited column names, or `null` for the whole row |
| `whole_file` | `{"kind":"whole_file","version"}` | |
| `page` | `{"kind":"page","version","page"}` | `page` is a 1-based int |
| `panel` | `{"kind":"panel","version","panel"}` | `panel` is a name registered by `file.panels_set` |
| `quote`, `page_region`, `image_region` | `{"kind",...}` | Reserved (11.13); stored as given and not interpreted; a claim carrying one is `not checkable` for that citation |

CLI spelling: `F7@v2` is `whole_file`; `F7@v2#<column>=<value>` with one `#<column>=<value>` per key column in the spec's key order is one `cells` key tuple (repeat the whole handle for more tuples, or give a file with `--cited-keys`); `F7@v2#page=3`; `F7@v2#panel=B`. Inside a value, `\#`, `\=`, and `\\` escape the three special characters; values are parsed per the column's type. When the cited version has a parse spec, every `#<column>=<value>` segment names a key column, the segments name every key column exactly once and in key order, and `page` and `panel` are ordinary column names; without a parse spec the only forms are the bare handle, `#page=<n>` with `n` a decimal integer from 1 without leading zeros, and `#panel=<name>` with a non-empty name. A value is read in the 11.6 `v` form of its column's type (`true` or `false` for `bool`; a `float64` column as `dec`). The CLI spelling has no column selector, so the `cells` citation it produces has `columns: null`. Any other input, an unknown escape included, is `E6007` with `field` `null`. In the JSON form, `keys` is a non-empty list and each tuple a non-empty list of typed values, none `missing`, `nan`, `inf`, or `list`; `columns` is `null` or a non-empty list of column names; `page` is a JSON integer of at least 1; `panel` is a non-empty string; an absent, extra, or unknown member is `E6002` and a wrong JSON type `E6004`.
