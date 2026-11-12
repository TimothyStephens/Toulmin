# Review: PR #2 (p2-record) at 836c0ad, design decisions

Reviewer session, 2026-11-12. Scope: the design decisions in the nine commits
362de64 … 836c0ad, not code quality. The test suite was not run and this
sandbox has no `cargo`, so nothing was built. **[Certain]** means I read it or
ran a read-only check this session; **[likely]** is a strong inference;
**Guessing** is marked where it occurs. Design section numbers are those of
`docs/design/toulmin_v1_design.md` at HEAD.

The numbered list of the ten decisions is in the builder's P2 report, which
is not in this clone. I number them from `docs/decisions.md`'s 2026-11-11 P2
entries in the order that fits the task's anchors (1 torn tail, 2 fencing,
5 sealing resume, 9 `trust_ctime`): 3 segment numbering and canonical small
files, 4 the initial `HEAD` and event positions, 6 the verify findings list,
7 manifest walk and exclude grammar, 8 the racy marker, 10 `gen-chain`
regeneration. Each is named by content below, so a different numbering does
not change a finding.

## Verdict

**Do not merge yet. One finding blocks (B1).** The rest of the rework is
sound: the two hand-walked second-writer cases hold, the manifest fingerprint
ruling is applied correctly, the catalog codes are consistent everywhere, and
every confirmation the task asked for passes except the one that needs
`cargo`. B1 needs two sentences in the design (3.1 and 11.21) and a small
change in `lock.rs` and `append.rs`; no frozen fixture changes.

## B1 (Blocker): a crash between step 0's seal and step 3's `HEAD` rename wedges the record

**Where.** 3.1 Torn tail (decision 1), 11.21 step 0 and Sealing (decision 5);
7191d4d `crates/tm-record/src/append.rs:270-279` and `:296-313`,
`crates/tm-record/src/lock.rs:417-446`, `crates/tm-record/src/segment.rs:198-205`
and `:308-338`, `crates/tm-record/src/verify.rs:160-172`.

**The sequence [Certain, by reading].** `append` runs step 0 before step 1:
when the batch would overflow, `segment::seal(N)` packs `seg-N.jsonl`,
verifies the packed copy, and **removes the plain file**; then the batch is
written to `seg-(N+1).jsonl` and synced; only in step 3 does `HEAD` move to
`{segment: N+1, offset: framed_len}`. If the process dies after the removal
and before the rename (a login-node kill during the batch write is the
design's own example of an ordinary crash), the state is:

- `HEAD = {segment: N, offset: L, …}` where `L` is segment N's full length;
- segment N exists only as `seg-N.jsonl.zst` (uncompressed length exactly `L`);
- `seg-(N+1).jsonl` holds one complete, valid, uncommitted batch.

What happens next:

1. **Every later lock fails.** `finish_lock` computes the open segment's
   length with `plain_len(&main, head.segment)`, which returns `0` when the
   plain file is absent (`segment.rs:198-205`). `0 < L` takes the
   `Ordering::Less` arm and returns `E2103 head_offset_mismatch`, and the
   cleanup removes the lock file (`lock.rs:436-444`). `tm lock break` goes
   through the same function. No `tm` command that takes the lock can run
   again, and nothing truncates the orphan in segment N+1.
2. **`tm audit` calls it a fork.** `verify` loads segment N from the packed
   form, finds `HEAD.offset == L`, walks clean, then reports
   `head_field_mismatch` for "segment N+1 exists beyond HEAD.segment"
   (`verify.rs:160-172`), a finding whose code is `E2102`. 3.1 says a fork
   is "corruption, not a torn tail … nothing truncates it".
3. Readers are fine: `Scan` and `read_at` load the packed segment and stop at
   `HEAD.offset`.

**Why this is a design finding, not only a bug.** Decision 1's rule is
"every byte of the open segment beyond `HEAD.offset` is a torn tail", and its
stated reason is that treating a crashed batch as a fork "would wedge the
record after an ordinary crash". The seal case escapes the rule twice: the
uncommitted bytes are not in the open segment, and the open segment no
longer has a plain file, so 3.1's "`HEAD.offset` beyond the end of the open
segment is corruption" is satisfied by a healthy packed segment. 11.21 step 0
describes the seal and says nothing about a crash after it. The Risks row
"The lock holder dies mid-batch" promises the torn-tail rule as mitigation;
in this window it does not apply. The window recurs once per 64 MiB and lasts
one batch write plus two `fsync`s.

**Fix (design first, then code, no fixture change).**

- 3.1 Torn tail: add "The open segment's length is that of whichever form
  exists (a packed segment's uncompressed length). A plain segment numbered
  `HEAD.segment + 1` is uncommitted and is removed at lock time as a torn
  tail, recorded as `chain.torn_tail_truncated` with that segment number,
  offset 0, and its length."
- 11.21 step 0: add "A sealing may have completed for a segment `HEAD` still
  names (a crash before step 3); the writer then rolls to the next segment
  without sealing again."
- `lock.rs` `finish_lock`: compute the open length from `segment::load` (or
  a `packed_len`) when the plain file is absent; after the existing
  truncation, if `forms(main, head.segment + 1).0` is true, remove that file
  and set `torn_tail = Some(TornTail { segment: head.segment + 1, offset: 0,
  bytes: len })`.
- `append.rs`: the fence's length check uses the same open-length rule; step
  0 skips `seal` when segment N is already packed-only and just rolls.
- `verify.rs`: a plain `seg-(HEAD.segment+1)` with `HEAD.segment` packed and
  intact is `torn_tail_beyond_head`, not `head_field_mismatch`.
- A `chains/crash_after_seal/` case (regenerated by `gen-chain --conformance`
  like `torn_tail_truncated`), expected to verify clean after the lock-time
  truncation and to carry one `chain.torn_tail_truncated`.

The 8-case tamper set of item 41 stays as it is.

## M1 (Major): a manifest walk that skips every symbolic link drops Nextflow-style published outputs

**Where.** Item 28 (decision 7), 11.16 `manifest`; 7191d4d
`crates/tm-fsfacts/src/manifest.rs:220-229`.

**What.** "Regular files are entries and subdirectories are walked; symbolic
links … are not entries, so a link cycle cannot occur." A directory whose
files are symbolic links to regular files therefore has an empty entry list
and the fingerprint of `[]`. Nextflow's `publishDir` default mode is
`symlink` [likely], and the design names Nextflow as an expected workflow
tool (Problem; 5.5's default ignore list carries `.nextflow/`; Future
Improvements). At gate C, CUJ 2's `results/de/` published that way would
version as empty, the run check would report "declared output not
produced", and nothing downstream would have lineage.

**Why it forecloses.** `entries` is frozen in `file.version_added` bodies
from P3 on. Changing the walk rule later changes the fingerprint of every
existing directory version that held links, which shows as `content changed`
and as `E3401` at `tm audit`, the same class of damage the owner's
decision-10 ruling just avoided (5.6 content labels, D22).

**Fix.** Rule it now, one of two ways, and state it in item 28:

- (a) A symbolic link whose target is a regular file is an entry: `path` is
  the link's relative path, `fingerprint` and `snapshot` are the target's
  (`statx` without `AT_SYMLINK_NOFOLLOW`). A link to a directory is not
  walked (no cycle), a dangling link is not an entry. Entry shape unchanged.
- (b) Keep the rule and say so where users will meet it: item 28, the user
  guide for clusters (gate C), and a `tm doctor` or `analysis.declare`
  warning when a declared output directory holds links.

I recommend (a).

## The three hand-walked sequences

The sequence at HEAD [Certain, from `lock.rs`, `append.rs`, `head.rs`]:
lock = `O_EXCL` create, read `HEAD`, write holder with `epoch+1` in place,
`HEAD` rewritten with the new epoch by write-then-rename with the lock
re-read before the rename, truncate beyond `HEAD.offset`; append = fresh
`HEAD` read (`HEAD.epoch == lock.epoch`, plain length `== HEAD.offset`,
batch continues `HEAD`), step 0, steps 1 and 2 with `fsync`, step 3 with the
lock re-read.

| Case | Walk | Outcome |
| --- | --- | --- |
| Crash between step 2 and step 3, no seal in step 0 | Bytes synced, `HEAD` unchanged. Readers stop at `HEAD.offset`. Next command waits out the heartbeat, takes over, writes epoch `e+2` to `HEAD`, finds `plain_len > HEAD.offset`, truncates, queues `chain.torn_tail_truncated` first in its batch (`tests/lock.rs` shows the Core-side queueing). | **Holds [Certain].** Nothing acknowledged is lost; the fact lands on the chain. |
| Crash between step 2 and step 3, seal in step 0 | See B1. | **Fails:** record unlockable, audit says fork. |
| Two writers with different epochs | W1 holds epoch `e`; W2 cannot create the lock and waits; a changing heartbeat never allows takeover (`E2201` after `wait`). After a stale heartbeat W2 removes the lock, creates its own, writes `e+1` to `HEAD`, truncates any tail. W1 waking before its step 2: `HEAD.epoch e+1 != e` → `E2102`. W1 waking between 2 and 3: its bytes were already truncated; the step-3 lock re-read sees epoch `e+1` → `E2102`, `HEAD.tmp` removed. W1 waking after 3: nothing pending; `release` compares inodes and leaves W2's file. | **Holds [Certain].** The equality rule (decision 2) is what catches a stale `HEAD` rename that would leave `HEAD.epoch` *lower* than the holder's; the old "higher only" rule would not. |
| Stale writer that held the lock before a takeover and wakes after it | As the previous row's three W1 cases. `tests/lock.rs::paused_holder_is_fenced_out` covers the first (append with an epoch-1 lock after an epoch-2 takeover → `E2102`); `head.rs` has the unit test for the lock re-read. | **Holds [Certain].** The heartbeat thread of the stale writer writes through its own descriptor to the unlinked inode, never to the taker's file. |

Residuals, both nits (N1, N2 below): the step-3 re-read and the rename are
two operations, so a takeover that lands exactly between them ends either
with W1's complete batch committed under W2's `HEAD` (benign) or with W1's
batch truncated and recorded while W1's caller saw success; and two waiters
on one stale lock can each pass the "still what I observed" check, after
which `finish_lock`'s cleanup removes the lock path without an inode check.
In every interleaving I walked, `HEAD.epoch` fencing keeps the chain from
forking; the costs are a recorded truncation or a spurious `E2102` retry.

## The ten decisions

| # | Decision | Consistent with the design? | Forecloses a later phase? |
| --- | --- | --- | --- |
| 1 | Everything beyond `HEAD.offset` is a torn tail; `HEAD` is the commit point | Yes for the open segment: it closes the gap the old text left (a complete batch beyond `HEAD.offset`) and matches 11.21's step 3 and the Risks row. **Incomplete for the seal window (B1).** | No, once B1's two sentences are added. |
| 2 | `HEAD.epoch` compared for equality; lock re-read before the `HEAD` rename | Yes. Strictly safer than "higher"; the re-read narrows the paused-writer window to one syscall pair. 5.3's "Prevent" wording overstates it by that pair (N2). | No. |
| 3 | Segments from `000001`; canonical-JSON header, `HEAD`, `lock`, `record.json` plus one newline; `b` from 1; header carries the writer's epoch | Yes. One encoding per file, so a hand edit of a header is `segment_byte_changed` (`BatchHeader::parse` re-serializes and compares). | No. `Head::read` treats `format != 1` as `E2103`, which collides with "opens for reading" if a future record format ever moves `HEAD.format` (N5). |
| 4 | Initial `HEAD` has `last_id` = genesis; positions are `(segment, byte offset of the line in uncompressed bytes)`; 11.17 holds them | Yes. 11.2 already says the first event's `prev` is the genesis string; the initial `HEAD` removes the special case. Positions survive packing. | No. |
| 5 | Sealing resume: both forms present → plain authoritative, packed verified or redone; readers prefer plain | Yes for the state it names (crash inside `seal` before the removal): `resume_seal` and a re-seal in the same append both heal it, and a stale packed copy is overwritten at the eventual seal. **It does not name the crash after the removal (B1).** | No, with B1. |
| 6 | Verify returns a list of `Finding`s; `head_field_mismatch` added; fixed check order | Yes. The order (count, IDs, payload hash, links) gives each tamper case one name; the implementation runs links after a payload-hash failure instead of stopping, which only matters for a double fault. | No. |
| 7 | Manifest walk: regular files only, no links; path form; exclude grammar; directory `size` = sum of entries | Walk rule: **see M1.** Exclude grammar matches the default list's needs; the parser refuses a multi-component directory pattern (`a/b/`) that the grammar does not define (N4). `size` as the sum is the natural reading of 11.16's "which bytes it counts is item 28's". | M1 for the link rule; otherwise no. |
| 8 | Racy marker placed in a caller-named directory on the same filesystem; compared at filesystem precision | Yes. Matches the Risks row's one-clock argument; `>=` is "at or after". | No. |
| 9 | `hashing.trust_ctime` has git's meaning; the `Unchanged`/`HashNow`/`Unconfirmed` table | Yes against 6.2 and CUJ 7: see the section below. The cost of `false` is unstated (N3). | No. |
| 10 | `gen-chain --conformance` regenerates `conformance/record/` with a small segment limit | Yes. Readers do not depend on the limit, and packed segments cannot otherwise appear in a small fixture. The cases are regenerable rather than independently written, which is weaker than the P1 cases' status; a CI reproduction check would make the generator a test of the writer (N6). | No. |

Three further decisions logged in the build commit: `Record::append(&Lock,
&Batch)` without an `OwnerCap` until P3 (consistent with the phase line;
rule 4's argument is owed at item 43, and it is logged); the manifest
fingerprint left unbuilt pending the collision ruling (correct under the
collision rule, now ruled); two codeless conditions reported rather than
invented (correct, now ruled).

## Decision 5 against a crash mid-seal

`seal` writes `seg-N.jsonl.zst.tmp`, syncs, reads it back and compares,
renames into place, syncs the directory, removes the plain file, syncs again
[Certain]. Crash points:

- before the rename: a stray `.tmp`, re-created next time; harmless;
- after the rename, before the removal: both forms, plain authoritative;
  the next append either re-seals N (if the new batch also overflows) or
  appends to the plain file, and the stale packed copy is replaced at the
  eventual seal; readers prefer plain throughout; `resume_seal` covers
  segments below `HEAD.segment` though that state cannot arise there;
- after the removal, before the batch's `HEAD` rename: **B1**.

So decision 5 holds for the two states it was written for and misses the
third.

## Decision 9 against 6.2 and CUJ 7

CUJ 7's hand edit to `config/de.yaml` reaches `tm status` as a `Scan` with a
previous snapshot: the edit changes `mtime` or `size`, `decide` returns
`HashNow`, the fingerprint differs, and Files records a version with origin
`changed_outside_run` (6.2 Origin; item 77). A `touch` is `HashNow` with the
same fingerprint, so no version: 6.2's `metadata only` is for run outputs and
does not apply to inputs. A large file is `Unconfirmed`, which is 5.6's
`possibly changed (unconfirmed)` with its named clearing actions. `Audit`
always hashes, which is G13. All consistent [Certain].

With `trust_ctime: false`, a same-size rewrite that restores `mtime` on the
same inode (`cp -p` over an existing file, some editors) is `Unchanged` in
both `Scan` and `Run`, so the wrapper would reuse a recorded hash for an
output whose bytes changed, until the next audit. That is git's trade-off
and an owner opt-in, but the design does not say what `false` gives up and
5.5's table of weaker options does not list it (N3).

## The manifest fingerprint as built in f3b7af9, against 5.6 and D22

`Manifest::fingerprint` digests the canonical JSON of
`[{"path","fingerprint"}]` in 11.10 order; `to_json` emits
`{"fingerprint","ignore","entries"}` with snapshots inside `entries` only
[Certain].

- **Touched directory:** same paths, same per-file fingerprints, so the same
  manifest fingerprint; Files sees the entries' snapshots changed and the
  fingerprint equal, which is exactly the information 5.6 needs for
  `metadata only` ("snapshot changed, content did not") or `carried forward`.
  D22's "identical content stops the spread" holds. Yes.
- **Renamed entry:** a different `path` string in the list, so a different
  fingerprint and `content changed`; the diff names it as one added and one
  removed. Yes, and right: the same bytes under another path are not the
  same output to a consumer.
- `ignore` is outside the digest, so two directories with the same resulting
  entries agree whatever list produced them. Fine.
- The item 31 case asserts all three (equal with different `mtime` and
  `ino`; different after one byte; different after a rename) [Certain, from
  reading `tests/fsfacts.rs:182-229`].

Matches the 11.16 row and the dated note under decision 10 of
`decisions-p1.md`. Correctly applied.

## The ruling commits: correctness of application

| Commit | Ruling | Applied correctly? |
| --- | --- | --- |
| 7434b6d, f3b7af9 | Manifest fingerprint over `[{path, fingerprint}]` | Yes: 11.16 row, items 28 and 31, dated note in `decisions-p1.md`, `docs/decisions.md` entry, `formats.md` regenerated; code and test as above. Design commit precedes the code commit. |
| 79c2797, 4067235 | `E2007` no `record.json`, `E2008` newer record format | Yes: section 9 (`project.init` with `--attach`, `read_in`), 11.12, item 73, NFR Rollout; catalog entries gate A; `docs/errors.md` rows; `Record::open` returns `E2007`, `writable()` returns a catalog `E2008` naming both formats, the `NewerFormat` variant is gone, and the unit test asserts both. The departure from the ruling's `E2006` is explained (taken by a settled P1 code) and logged. Design commit precedes the code commit. One looseness: section 9 lists `E2007` only on `project.init`, while 11.12 and item 73 say any command that opens a record; harmless since per-command resolution is not a section 9 row. |
| f9a791d | Runbook section 6 row for `COLLATE BINARY` | Yes: deferred to P4 with the two acceptable outcomes stated; 11.10 is checker.md's and unchanged, which the runbook's "do not fix them now" rule requires. |
| df9ba42 | Dev-profile override `[profile.dev.package.libduckdb-sys] debug = false` | Yes: dev only (the test profile inherits it, which is the point); release profile, item 5's lint levels, and D24 untouched; logged. |

## 836c0ad, the two checks

- **deny.toml additions:** `fastrand`, `tempfile`, `zstd`, `zstd-safe`,
  `zstd-sys` [Certain]. `zstd` and `tempfile` are named in item 1;
  `zstd-safe` and `zstd-sys` are `zstd`'s own crates and `fastrand` is
  `tempfile`'s dependency. Under the P0 decision that the allow list is every
  crate in `Cargo.lock`, all five are permitted. (7191d4d's earlier additions,
  `rusqlite` and its tree, are item 1's `rusqlite`.)
- **ci.yml:** the only change is a `rm -rf target/duckdb-sources
  target/*/duckdb-sources` step after each `rust-cache` restore, with an `if:`
  only in the two jobs whose neighbouring steps already carry the same
  condition [Certain]. No check is removed, weakened, or made conditional.
  The `Cargo.toml` change pins the two path dependencies at `=0.1.0`, which
  item 4's wildcard ban wants.

## Confirmations

| Check | Result |
| --- | --- |
| Every design edit precedes the code that depends on it | **Holds [Certain].** 362de64 (design only) → 7191d4d (code, `docs/decisions.md`, `deny.toml`); 7434b6d → f3b7af9; 79c2797 → 4067235. No code commit touches `docs/design/`. The one thing 7191d4d did without design text, `E2004` for a missing `record.json`, was logged as pending a ruling and replaced by 4067235 after 79c2797. |
| The eight tamper cases are item 41's list minus `verdict_data_changed`, nothing else | **Holds [Certain].** `conformance/record/tamper/` holds exactly `batch_hash_mismatch`, `fork_prev_mismatch`, `head_offset_mismatch`, `kept_byte_changed`, `packed_segment_corrupt`, `segment_byte_changed`, `segment_line_removed`, `torn_tail_beyond_head`, each `expected.json` naming one finding equal to its directory; `record_conformance.rs` asserts the set equals `FindingKind::ALL` minus `HeadFieldMismatch`. |
| `tm-record` has no dependency on `tm-index` | **Holds [Certain].** `crates/tm-record/Cargo.toml` depends on `tm-formats`, `tm-fsfacts`, `zstd`, `rustix`, `serde_json`, `sha2`; no `tm_index` in its sources. `scan.rs` reads by position and scans linearly by ID. |
| No journal or inbox API was built | **Holds [Certain].** `layout.rs` creates `main/`, `objects/sha256/`, `objects/tmp/` only; the words journal and inbox appear in two doc comments deferring them. The public surface is listed in `lib.rs` and holds no such function. |
| `spec-drift.sh` passes at HEAD for `record.md` | **Holds [Certain]:** `OK docs/spec/record.md matches 3, 11.21`; `formats.md` also OK. The script's third check, `docs/errors.md` against `build.rs` output, is **unverified here** (no `cargo`; the script exits 1 for that reason alone). By hand, the 76 codes and gates in `catalog.rs` equal the 76 rows of `docs/errors.md` [Certain], so CI's byte comparison passes [likely]. |
| `E2007` and `E2008` in section 9, 11.12, the catalog, `docs/errors.md`, gate A | **Holds [Certain].** Section 9 rows `project.init` and `read_in`; 11.12's new sentence; `catalog.rs:32-33` with `'A'`; `errors.md` rows 19 and 20 with gate A. |

## Nits

- **N1** `lock.rs:334-343` and `:398-401`. The stale-lock break is read, compare, `remove_file`; two waiters can both pass the compare, and the loser's `finish_lock` cleanup removes whatever is at the lock path, possibly the winner's file. `HEAD.epoch` fencing keeps the chain safe in every interleaving I walked; the cost is a lockless window and a spurious `E2102`. Fix: break by `rename` of the stale file to a side name before `O_EXCL` create, and make the cleanup inode-checked like `remove_if_ours`.
- **N2** 5.3 rows "Two `tm` commands write at once" and "Paused writer wakes after takeover" say "Prevent". The step-3 re-read leaves one syscall pair in which a takeover yields either W1's batch committed under W2's `HEAD` or W1's batch truncated and recorded while W1's caller saw success. Say "prevent, or record as a truncation" so the column matches the mechanism.
- **N3** Item 30 and 11.19: state what `hashing.trust_ctime: false` gives up (a same-size, `mtime`-preserving rewrite on the same inode is not seen by `tm status` or the wrapper until the next audit), and list it in 5.5 as a weaker option.
- **N4** Item 28's exclude grammar does not define a directory pattern with more than one component; `Excludes::parse` refuses `a/b/` with an I/O error, which `settings.set` would surface as `E7002` under 11.19's "each entry parses as a glob". Either say that a directory pattern is one component, or define `a/b/` as a whole-relative-path directory match.
- **N5** `head.rs:89-94` maps `HEAD.format != 1` to `E2103`. If a later record format also bumps `HEAD.format`, a newer record cannot be read at all, against 11.3 and NFR Rollout's "opens for reading". Decide now whether `HEAD.format` follows `record.json`'s `format`; if it may, read-only open should accept a higher one it can still parse.
- **N6** Item 40: add a CI step that runs `gen-chain --conformance` into a temporary directory and diffs it against `conformance/record/`. The cases are only as frozen as the generator; the diff turns every writer change that alters bytes into a visible review item, which is what the formats cases give P1.
