# Re-audit: PR #2 (p2-record) at d433a56, against the 2026-11-12 review

Reviewer session, 2026-11-12. One pass, scoped to whether each finding of
`reviews/2026-11-12-p2-record-design-decisions.md` is closed. **[Certain]**
means I read it or ran a read-only check this session; **[likely]** is a
strong inference. No `cargo` here, so no test ran; every "the test covers
it" below is the test as written, not as executed.

## Verdict

**Merge.** B1 and M1 are closed as ruled; N2 to N6 are closed; N1 is closed
with one new gap in the race it was written for (N1b below), which loses an
audit event but never the chain. N1b is a ten-line fix and can land before
or after the merge at the owner's choice.

## Commit discipline

Eight commits, three design and five code, each design commit before the
code that depends on it [Certain]: 5269f2f → cfde322 (B1); e2fe01b → 9de89ef
(M1); 1c1b7cc → a9adea3 (N1), c9a627e (N5), d433a56 (N6). No code commit
touches `docs/design/`. `docs/decisions.md` gains eight dated entries, one
per finding. `spec-drift.sh` passes for `formats.md` and `record.md` at HEAD
[Certain]; the `errors.md` half is unverified here, and nothing in these
eight commits touches the catalog or `docs/errors.md` [Certain], so its
status is what it was at 836c0ad. The eight tamper cases and the nine
existing chain fixtures are byte-identical to 836c0ad; the only fixture
change is the new `chains/crash_after_seal/` [Certain, `git diff --stat`].

## Findings

| Finding | Status | Why |
| --- | --- | --- |
| **B1** crash between the seal and the `HEAD` rename | **Closed** | 3.1 states the open-length rule, the orphan `HEAD.segment + 1` as a torn tail with its `chain.torn_tail_truncated` shape, audit's `torn_tail_beyond_head`, and that a plain-beside-plain state stays `head_field_mismatch`; 11.21 step 0 states the roll without re-sealing. Code: `segment::open_len` (plain, else packed uncompressed length, else 0); `truncate_torn_tail` in `lock.rs` truncates a plain tail, refuses a packed open segment longer than `HEAD.offset`, and removes a plain `HEAD.segment + 1` after a packed-only `HEAD.segment`, queuing `TornTail {segment: N+1, offset: 0, bytes}`; the append fence uses `open_len`; step 0 rolls when `HEAD.segment` is packed-only and refuses with `E2103` if the next segment already exists; `verify` names the state `torn_tail_beyond_head` when the open segment is packed-only and clean. The walk I did for the blocker now ends at a successful lock, one truncation event, and a roll [Certain, by reading]. `tests/lock.rs::crash_after_seal` asserts all of it; `chains/crash_after_seal/expected.json` carries one `chain.torn_tail_truncated` among six events, segment 1 packed, segment 2 plain, findings empty. |
| **M1** symbolic links in manifests | **Closed** (option a) | Item 28 and 11.16 state the rule; `walk` resolves a link with `metadata`, makes a file target an entry under the link's path, does not walk a directory target, skips a dangling one; `Snapshot::take` follows the final link and hashing opens the path, so the entry carries the target's facts. The item 31 case covers a relative-target link, a directory link that would be a cycle, a dangling link, and a directory of links only; the exclusion case now expects the link and sees it leave when its target does. |
| **N1** stale-lock break | **Closed, one new gap (N1b)** | The taker renames `main/lock` to `main/lock.stale` before its `O_EXCL` create, so exactly one waiter breaks a file; `finish_lock` removes the side file once its own lock is written and its cleanup is inode-checked. 3.1 says so. `paused_holder_is_fenced_out` asserts the side file is gone. |
| **N2** 5.3 wording | **Closed** | Both rows read "Prevent, or record as a truncation (the one-syscall-pair window named in 3.1)", and 3.1 names the window with three outcomes. The builder added a third outcome I had not listed, a stale `HEAD` that audit reports, and left closing the window (per-writer `HEAD.tmp` names) to the owner. That is the right call: it is a mechanism change, not a wording fix. |
| **N3** `trust_ctime: false` cost | **Closed** | Item 30, 11.19, and a new 5.5 row say what `false` gives up, in `Scan` and `Run`, until the next audit. |
| **N4** directory exclude pattern | **Closed** | Item 28: exactly one component followed by `/`; `Excludes` refuses anything else; `settings.set` reports `E7002`. Code unchanged, as the parser already behaved so. |
| **N5** `HEAD.format` | **Closed** | 11.3: `HEAD.format` follows `record.json`'s and never exceeds it; a read-only open accepts a higher one whose seven fields parse; a writable record with another format, or a `HEAD.format` above `record.json`'s, is `E2103`. `Head::read_with(main, max)`; `Record::head()` passes `max(record.json format, 1)` and is the readers' path (`events`, `events_after`, `read_at`, `verify`); writers keep `Head::read` at format 1. Unit tests cover the parser and both sides of the open. |
| **N6** fixture regeneration check | **Closed** | Item 40 states it; the conformance job runs `gen-chain --conformance` into `$RUNNER_TEMP`, drops empty directories, and `diff -r`s against `conformance/record/`. |

## N1b (Nit): in the two-waiter race, the takeover event is lost

`lock.rs:393-428` [Certain]. W2 and W3 both wait on a stale lock. W2 wins
the rename and sets `taken_over = Some(holder)`; before W2's `O_EXCL`
create, W3's loop reaches the create first and wins it. W2 then finds a lock
present and clears `taken_over` (the comment at `:400-403` describes this
case). W3 never renamed anything, so its `taken_over` is `None`, and its
`finish_lock` deletes `lock.stale` without reading it. The epoch increments
but no `lock.taken_over` event is queued, against 3.1 Takeover ("records a
`lock.taken_over` event"). The chain is safe; the audit trail of who was
displaced is not.

Fix: in `finish_lock`, before removing `main/lock.stale`, read it; if it
parses as a holder and `taken_over` is `None`, use it as the displaced
holder. The side file is exactly the broken lock's content, so the record
is complete whichever waiter wins the create. One assertion in a
two-waiter variant of `paused_holder_is_fenced_out`.

## Addendum at 1611ec1: N1b and the naming nit

Four commits after d433a56, design before code in both pairs (b328a3f →
aa744dc; f6d4ef9 → 1611ec1); no code commit touches `docs/design/`;
`spec-drift.sh` passes for `formats.md` and `record.md`; no fixture byte
changed; two dated entries in `docs/decisions.md` [Certain]. Tests not run
here.

| Item | Status | Why |
| --- | --- | --- |
| **N1b** displaced holder lost in the two-waiter race | **Closed** | 3.1 Takeover: the taker reads `main/lock.stale` before removing it and, when it parses, that holder is the displaced one whichever waiter won the rename. `finish_lock` reads the side file, adopts it as `taken_over` when it renamed nothing itself and the content parses (epoch 0, the malformed marker, is skipped), then removes it. A leftover side file from a taker that crashed between its rename and its create is attributed by the next taker, which is the correct attribution since that taker is the one that increments the epoch. `two_waiters_split_rename_and_create` drives the race by hand and asserts one `lock.taken_over` with `previous.epoch` 1 and `epoch` 2, no second record when the renamer later locks, and a clean verify. |
| Naming nit, packed open segment longer than `HEAD.offset` | **Closed** | 3.1's torn-tail paragraph names it `head_offset_mismatch` with the reason; `verify_segment` reports that kind when the loaded form is packed, the plain case keeps `torn_tail_beyond_head`; the lock path was already `E2103 head_offset_mismatch`. The unit test builds the state by tampering and checks both paths. Item 41's set is untouched. |

Nothing is left open from the P2 review or the re-audit. **Merge.**

## Other observations from the rework hunks

- `verify_segment` still reports `torn_tail_beyond_head` for a packed-only
  open segment whose uncompressed length exceeds `HEAD.offset`, while
  `truncate_torn_tail` refuses that state with `E2103 head_offset_mismatch`
  [Certain]. The writer never produces it (a seal happens only when the
  plain length equals `HEAD.offset`), so it is reachable by tampering only;
  the two names should agree, and `head_offset_mismatch` is the right one
  since nothing truncates a packed segment. Nit.
- `append` now refuses to roll onto an existing next segment with `E2103`.
  3.1 names that state `head_field_mismatch` for audit; the lock-time path
  does not remove it. Consistent with "corruption, nothing truncates it".
