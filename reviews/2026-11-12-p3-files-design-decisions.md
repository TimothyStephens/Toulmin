# Review: PR #3 (p3-files) at 745cbe0, design decisions

Reviewer session, 2026-11-12. Scope: the design decisions in the six commits
a3fd86f … 745cbe0, not code quality. The test suite was not run and this
sandbox has no `cargo`, so nothing was built. **[Certain]** means I read it
or ran a read-only check this session; **[likely]** is a strong inference.
Design section numbers are those of `docs/design/toulmin_v1_design.md` at
HEAD. I read the design in full at the P2 head earlier this session and
every diff since, including the three P3 design commits.

The ten decisions are the first ten P3 entries of `docs/decisions.md`
(2026-11-12), in their order: 1 tests and generator through the
crate-private append; 2 the input path form; 3 an equivalent candidate adds
no version; 4 the transition table; 5 the reading rules; 6 the cache path;
7 the image signatures; 8 the fold rows; 9 `samples.tsv`'s key and the
`expected.json` stub; 10 NaN tokens, checksum file rules, `args.txt` form.

## Verdict

**Do not merge yet. One finding blocks (B1), and the unruled decision (M1)
should be ruled with it, since the same fix settles both.** Everything else
holds: the three rulings are applied correctly, the manifest regeneration
touched exactly the two P1 cases, the ten fixture claims are determinate
with exactly one intended failure, and every confirmation passes except the
`docs/errors.md` half of spec-drift, which needs `cargo` and is unverified
here (catalog and table both have 78 rows and the two new codes [Certain]).

## B1 (Blocker): "equivalent adds no version" leaves a pinned fingerprint the disk no longer matches

**Where.** Decision 3, item 45; `crates/tm-files/src/version.rs:293-302`
(`Version::next` returns `None` for an equivalent candidate). Collides with
7.3, item 80 (`E3401`), 11.21 Objects (`E3203`), 3.4, and 5.6's clearing
action for `evidence moved`.

**The rule.** A candidate that is equivalent to the latest version under
any of the three equivalences adds no version. For the first equivalence
(same bytes) that is right and is what decision 9's note says: a touch is
no version. For the other two the bytes differ: a keyed table re-sorted or
re-spelled (`1.90` to `1.9`, the test's own case), or a `.gz` recompressed
at another level. The latest version keeps fingerprint `fp1`; the disk now
holds `fp2`.

**The walks the task asked for.**

| Case | Under decision 3 | Consequence |
| --- | --- | --- |
| Touched, unchanged | `SameBytes`, no version, stat cache reconfirmed | Right. |
| Changed, then changed back | v2 `content_changed` for the edit; the change-back differs from v2, so v3 `content_changed` with v1's fingerprint | Right. Versions are history; claims pinned to v1 see `evidence moved: newer version exists` and the owner accepts or not. |
| `.gz` recompressed at another level (same node) | `SameDecompressed`, no version | Wrong state, below. |
| Keyed table re-sorted | `SameTableByKey`, no version | Wrong state, below. |

**The wrong state [Certain, from the design text].** After a re-sort or a
recompression the record says the latest version is `fp1` and the disk is
`fp2`, with no event recording that this is fine:

- 7.3 defines "disk differs" as "the on-disk file no longer matches the
  pinned fingerprint". Every claim citing the version gets `evidence moved:
  disk differs`; a `cells` citation on an unkept version becomes `not
  checkable`. 5.6 clears `evidence moved` by `tm claim accept`, a rerun, or
  restoring the bytes; there is no newer version to accept onto, so the
  flag stays until the owner restores the old byte order.
- Item 80: `tm audit` re-hashes every tracked file and reports `E3401` for
  this one on every run, on an unchanged project.
- 11.21 Objects: a later `tm file keep` of that version streams the disk
  bytes, compares with the expected fingerprint, and fails with `E3203`.
- 3.4: the stat cache holds "the fingerprint it was confirmed against"; it
  can hold `fp1` (and re-run the parse-and-compare on every scan, since the
  stat differs) or `fp2` (and no longer describe the version). Neither is
  what 3.4 says.

The record's invariant everywhere else is that a version handle names bytes
with one fingerprint: `claim.checked.against`, kept objects addressed by
fingerprint, the cache key of decision 6, 7.3's columns. Decision 3 breaks
it for two of the three equivalences.

**Why the rule was chosen, and why the alternative is right.** The builder
read item 45's "new version if different content" through the three
equivalences and decision 9's note. But for run outputs the design already
does the other thing: a rerun with identical content *does* produce an
output version, labeled `carried forward` or `metadata only` (CUJ 2's
"14 outputs, 2 carried forward", 5.6, 8.4 "a rerun whose output version is
carried forward"), and the label, not the absence of a version, is what
stops the spread (D22). Inputs should work the same way: every byte-different
candidate is a new version, and the content label says whether the content
changed.

**Fix (design first, then code).**

- Item 45: "A candidate whose fingerprint equals the latest version's adds
  no version. One whose fingerprint differs is version `n + 1`:
  `content_changed` when it differs under all three equivalences, else
  `metadata_only` (same table by key, or same decompressed content; the
  bytes changed, the content did not). Origin is `changed_outside_run` for
  a hand edit found by a disk scan."
- 5.6 `metadata only` row: On "Output version" becomes "Version" with the
  reason "Snapshot or bytes changed, content did not"; 6.2 unchanged.
- Section 8, rule 2: "`own inputs changed` when any declared input's
  current version differs in content from the one the run used: a version
  labeled `metadata_only` or `carried_forward` counts as the version it
  equals". Rule 4 already says the same for outputs. CUJ 7 is unchanged: a
  hand edit that changes content is still a new version labeled so.
- 7.3 and item 80 then hold as written. Nothing in 11.16 changes:
  `content_label` already allows all three values.
- `Version::next`: drop the `equivalent` short-circuit for a differing
  fingerprint; take the `Equivalence` and set the label from it. `tests/
  files.rs::each_equivalence_rule`: the re-sort and the recompression each
  produce a version labeled `metadata_only`; the copy produces none.

The second node problem (M1) decides *where* the decompressed digest lives
so that this labeling works on every node.

## M1 (Major): the decompressed fingerprint in the node-local stat cache cannot label a version on a second node

**Where.** The unruled decision in 745cbe0 and 1fa1db7: item 45's last
clause, 11.17's `stat_cache.decompressed_fingerprint`; `version.rs:157-159`
and `:416` (`Version::from_added` sets `decompressed: None`, so a version
read back from the chain never has it).

**The walk.** Gate C, setup 2 (shared cluster, login nodes differ per
session, 2.2). Analysis A3 writes `results/x.tsv.gz`. Run R1's read-in on
login node N1 records `F9@v1` with fingerprint `fp1`; N1's stat cache gets
`d1`, the decompressed digest. R2 recompresses at another level: bytes
`fp2`, decompressed `d1`. The owner's next `tm status` runs on N2. The v1
bytes are not kept (D4: kept only when a promoted claim cites them) and are
gone from disk (overwritten). N2 must label v2. It has `fp1` from the chain,
`d2` from the disk, and no `d1`: the receipt (11.5) carries `hashes` only,
the `file.version_added` body carries no decompressed digest, and N1's stat
cache is on N1. So N2 cannot tell `carried forward` from `content changed`
[Certain, from 11.5, 11.16, 11.17, and `from_added`]. It labels v2
`content_changed`, A5 goes `upstream changed`, and G7's measure ("a rerun
that reproduces identical output clears the downstream flags") fails on the
setup the design is built for. The label is permanent (5.6), so it is also
node-dependent history. The same happens in gate A for a registered `.gz`
input recompressed by hand: a spurious version on N2, none on N1.

D22 does not accept this degradation: "identical content stops the spread"
has no node condition, and 3.4's stat cache is defined as a cache of
confirmations whose loss costs re-hashing, never a different answer.

**The alternatives and their cost.**

| Option | What | Cost |
| --- | --- | --- |
| (a) `file.version_added` tv 2 with `decompressed_fingerprint` (ID or `null`) | 11.16 lists tv 2 as tv 1 plus the key; `tm-formats` validates both; Files writes tv 2; the fold fills a `file_versions.decompressed_fingerprint` column; the receipt (11.5, gate C, not frozen) gains `decompressed` beside `hashes`, computed by the wrapper that already streams the output | The tv-bump machinery 11.11 and 11.16 were written for: two envelope cases, one 11.16 row, one 11.17 column, no change to any P1 case (tv 1 lines stay valid under tv 1). Every node then has the digest. The wrapper decompresses each `.gz` output once; that cost is intrinsic to D22's third equivalence wherever it is computed. About a day. |
| (b) A gate C event recording the equivalence when first established | Nothing in gate A; at gate C the read-in writes an equivalence event when it has both digests | It never has both on a second node: the earlier digest must come from somewhere durable, so (b) still needs the receipt or the body to carry it, and it does not cover gate A inputs (a hand recompression of a registered `.gz`). It is (a) with extra moving parts. |
| (c) Keep the stat cache placement | As built | Node-dependent labels and versions; G7 fails on setup 2; D22's third equivalence is effectively "same node only". |

Deferring (a) to gate C is bounded, not foreclosed: versions created before
the bump lack the digest, so each compressed input or output takes one
spurious `content_changed` version at its first candidate after the bump,
and `metadata_only` from then on. But P3 is where Files writes the body and
where the tv machinery is fresh; I recommend (a) now, in this PR. With (a)
the 11.17 `stat_cache` column is a second encoding of a chain fact and
should go; the stat cache keeps snapshot and fingerprint as 3.4 says.

## The ten decisions

| # | Decision | Consistent? | Forecloses? |
| --- | --- | --- | --- |
| 1 | `tm-record`'s lock cases and generator move inside the crate and append through the crate-private path | Yes. Rule 4 and item 8 leave no other way for a test outside `tm-cli` to hold a capability, and a test-only constructor would be a second one. Items 40 and 42 say so; the NFS job runs `--lib`. The P7 runbook row (tm-core's unit tests) is the right place to carry the open question. | No. |
| 2 | Input path form behind `E3001` | Yes: 6.1 makes the path the identity, so one spelling is needed; Files checks form, the Core checks existence. | No. |
| 3 | Equivalent candidate adds no version | **No, for the two byte-different equivalences (B1).** Right for same bytes. | Yes until fixed: every version created under it may be one whose fingerprint the disk no longer matches. |
| 4 | Transition table behind `E3201` | Mostly. Every section 9 file row's transition is present except **`file.hash`** (registered with no version, or `--again`; `E3005` "already hashed"): `E3005` appears nowhere in `tm-files` and the `Action` enum has no hash variant, so the one transition that turns a `--defer-hash` registration into a citable file lives outside the table (N1). Nothing in the table lacks a command: `Retire` is automatic by 6.1 and marked gate C. Two refusals the design does not state: `keep` after `keep --fingerprint-only` (upgrading to bytes) and `register` on an already registered identity (N2). | No. |
| 5 | Reading rules | Yes against 11.7 and 11.6; the scale rule is below. | No. |
| 6 | Cache path `<cache>/<file fp>/<spec fp>.parquet` | Yes. The key is content-addressed, so 3.4's locality is a placement choice with no correctness weight, and 11.17's `claims.evidence` re-run needs exactly (pinned version's fingerprint, the claim's `versions.parse_specs` fingerprint), which the key is. Write-then-rename makes a half-written file unreadable by name. | No. |
| 7 | Image signatures behind `E3301` | Yes; section 9's `file.panels` row. | No. |
| 8 | Fold rows | Yes against 11.17: `file.registered` upserts to `registered` (a tracked identity gaining a title), `file.restored` clears `retired`, versions insert-or-ignore, panels replaced. `file.retired` is gate C's. | No. |
| 9 | `samples.tsv` keyed by `(condition, batch)`; `expected.json` a stub | Yes: impl-M7 needed a composite key and six samples in two conditions and three batches give one without a fourth table; freezing verdicts before the Checker exists would freeze a guess. The README's prose answers are the right anchor (N3 on how P8 fills the stub). | No. |
| 10 | NaN and infinity tokens; checksum file rules; `args.txt` one argument per line | Yes: 11.6 allows `nan`/`inf` only in a column the spec maps, and `E3102` elsewhere unless listed as missing is the strict reading; item 50's forms are the two the design named plus the binary marker; the `args.txt` form answers impl-M7. | No. |

## Decision 5: the scale rule against 11.7

11.7 as P1 froze it: a `dec` column "carries `scale`, a JSON integer from 0
to 38 that `table.spec` records as the largest scale seen", and every key
shown is required. 11.6: "`s` fixed per column as the largest scale seen
when the parse spec is set; a value that does not fit at that scale is a
parse failure".

When "largest fraction seen" applies [Certain, `parse.rs:503-566`]: only in
`complete_spec`, the `table.spec` path. It reads every non-missing cell of
each `dec` column as text as written (trailing zeros count, per 11.6
"Parsed values keep the scale as written"), takes the longest fraction, and
writes that `scale` into the spec, overriding whatever the draft said. The
stored spec therefore always carries a scale, which is what 11.7 requires,
and its value is 11.6's. At every later parse (`typed_table`) the stored
scale is enforced: a cell with a longer fraction is `E3102` naming row and
column, and the scale is never recomputed. So after `table.spec` the rule
cannot disagree with the spec; before it, a draft that disagrees with the
data is corrected in the data's favour. Consistent.

Two observations, not defects: a draft scale *wider* than the data is also
overridden, so an owner cannot reserve room for a later version with more
digits and must re-run `table.spec` (a new spec fingerprint) when one
arrives; that is 11.6's rule and a usability choice the owner may want to
revisit (N4). A Parquet `Decimal128` column with a larger declared scale
than the spec's reads `1.2300` as scale 4 and fails `E3102`; 11.6 says scale
is "as written", so this is consistent, but item 47 could say it (N5).

## Decision 6 against 3.4 and 11.17

Covered in the table: the content-addressed key is what makes the cache
safe to lose, share, or place anywhere `tm doctor` allows, and it is the
pair `claims.evidence` must look up. One interplay with B1: under decision
3 a pinned version's bytes can be gone from disk while an equivalent file
sits in its place, and a missing cache entry for the pinned fingerprint
could only be regenerated by re-running the equivalence logic; under B1's
fix the pinned version's bytes are either kept, on disk, or 7.3's `not
checkable`, and the cache needs no such logic.

## The rulings in 1fa1db7 and 745cbe0

| Ruling | Applied correctly? |
| --- | --- |
| `tm-formats` manifest check catches up with decision 10's amendment | Yes. `check_manifest` digests `[{"path","fingerprint"}]` in entry order; the two P1 cases are regenerated (the valid line's ID changes, the invalid case's pointer `/body/manifest/entries/1/path` is unchanged); `git diff --stat` from the P2 head shows exactly those two files under `conformance/formats/` and nothing under `conformance/record/` or `conformance/checker/` [Certain]. The decision-log entry names the collision and the ruling. Python cross-check identical for 306 cases: the builder's claim, unverified here. |
| `flate2` and `md-5` on item 1's list | Yes: item 1 names both with their purposes and the restriction that every Toulmin fingerprint is sha256; `Cargo.toml` pins them; `deny.toml` follows the lock. `md5` is used only in `checksum::compare` for an MD5 list [Certain]. |
| `E3006`, `E3302` | Yes: section 9 (`file.register`, `claim.add`), 11.12, catalog gate A, `docs/errors.md`, and the two producing functions. Each is the next free code in its operation's block. `E3302` at `claim.add` time matches 7.3's add-time refusal pattern (`E6003`). |

Design before code holds for all three pairs: a3fd86f → 236f6ea, 65d7fa1 →
63d118f, 1fa1db7 → 745cbe0; no code commit touches `docs/design/` [Certain].
The P2 deviation (`append` without `OwnerCap`) is marked retired in its own
entry.

## The ten fixture claims under 11.18

Walked by hand on `de.tsv`, `samples.tsv`, `counts.tsv` [Certain]:

| Claim | Final table and assertion | Answer |
| --- | --- | --- |
| C1 | `padj < 0.05` keeps G01 G02 G03 G04 G05 G09 G10 G11 (G06 at 0.06 and G12's missing drop); sorted by `log2FoldChange` desc: G01 G02 G03 G04 G05 …; `take 5`; G05's position is 5 | holds |
| C2 | `padj < 0.01` keeps G01 G10 G11 (G04 at 0.0100 is not below); G10's value is `-2.40`; `eq -2.50` with `abs 0.01` needs a difference at most 0.01, it is 0.10 | **fails**, the one intended |
| C3 | `up = log2FoldChange > 0` is true for G01 to G07 and G12; `group by up`; `count` for the `true` row is 8 (`mean_lfc` 14.9/8 = 1.8625 exactly after 15-digit rounding); `row` is the bool key | holds |
| C4 | Inner join on `gene_id`; `ctrl_mean` is exact decimal division: G01 330/3 = 110, G11 900/3 = 300; `lt` | holds |
| C5 | Within `up` groups, rank by `baseMean` desc: G05 (200.0) and G10 (110.0) are the rank-1 rows; `filter rank = 1`; `exists` G10 | holds |
| C6 | `select` then `padj < 0.01`: three rows; `count eq 3` | holds |
| C7 | `whole_file` `F2@v1` exists | holds |
| C8 | page 2 of a two-page PDF | holds |
| C9 | panel `B` registered | holds |
| C10 | `cells` with the composite key in spec key order (`condition`, `batch`); `steps` null, so the final table is the cited row; `sample_id` of `(treated, 2)` is `S5` | holds |

Each has one answer; nine hold and C2 fails, as `README.md` says. The
report's ten spans match the ten sentences and the 11.22 ref form.
`expected.json` is `{"claims": {"1": null, …, "10": null}}` [Certain].

## Confirmations

| Check | Result |
| --- | --- |
| Design edit precedes the code | Holds [Certain], above. |
| `Record::append` takes `&OwnerCap`; P2 deviation retired | Holds [Certain]: `append(&self, _cap: &OwnerCap, lock, batch)`; the deviation entry carries "Retired, 2026-11-12 (P3)". |
| `check-owner-cap.sh` finds `new_for_cli` only in `tm-owner-cap` | Holds [Certain] by the script's own grep run here; its build half has no binary crate to build yet and would print the gate A no-op. |
| `MultiGzDecoder`, not `GzDecoder` | Holds [Certain]: the only decoder is `flate2::read::MultiGzDecoder` at `version.rs:12,201`; the `.bgz` test writes two members. |
| Ten fixture claims determinate, one fails | Holds, above. |
| `expected.json` is a stub | Holds [Certain]. |
| Spec-drift at HEAD | `formats.md` and `record.md` OK [Certain]; the `errors.md` half is unverified (no `cargo`); the catalog and the table both carry 78 rows including `E3006` and `E3302` [Certain]. |
| Manifest regeneration changed exactly two P1 cases | Holds [Certain]. |

## Nits

- **N1** Decision 4: add `Hash { again: bool }` to the transition table:
  allowed on a registered identity with no version, or with `again`;
  otherwise `E3005`. Say so in item 46.
- **N2** Item 46: state the two refusals the code makes that the design
  does not: `keep` after `keep --fingerprint-only` (no upgrade to bytes;
  `evidence not kept` is permanent anyway, 5.6) and `register` on a
  registered identity.
- **N3** Item 82a: say that P8 fills `expected.json` from `README.md`'s
  stated answers *before* the scenario tests run and then asserts the
  tests reproduce them, not from the tests' output; otherwise the file
  only freezes whatever P8's Checker said.
- **N4** 11.6 and item 47: an owner cannot reserve a wider scale than the
  data shows; a later version with one more fraction digit is `E3102` until
  `table.spec` is re-run, which changes the spec fingerprint every claim's
  `versions.parse_specs` records. If that is intended, say it in the user
  guide; if not, let `table.spec` take the larger of the draft's and the
  observed scale.
- **N5** Item 47: a Parquet decimal column whose declared scale exceeds the
  spec's reads `1.2300` as scale 4 and fails `E3102`; one clause saying
  Parquet decimals are read at their declared scale, as written.
- **N6** Item 47: a key cell that is `missing`, `nan`, or `inf` is refused as
  `E3102` (`parse.rs:448-455`); the design does not say what a missing key
  value is. State it (the refusal is right: a key tuple must be a value).
- **N7** D19 and item 45: `Candidate::of_file` decompresses every `.gz` or
  `.bgz` candidate it hashes, so a hashed compressed file costs two passes.
  Fine, but the hashing-time budget row should say so, since the wrapper
  will do the same for outputs under M1's option (a).
