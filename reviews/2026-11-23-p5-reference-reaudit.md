# Re-audit: p5-reference at 5875b79, against the 2026-11-23 design-decisions review

Reviewer session, 2026-11-23. Scope: whether each finding of
`reviews/2026-11-23-p5-reference-design-decisions.md` (reviewed at 8d4a5a2)
is closed, plus the checks the request lists, over the twelve commits
8d4a5a2..5875b79: cb6074f, ce3b5d3, 0c9f51b, f2055ae (the follow-ups);
6072a51 (deny); b48e8be and 4d82af7 (the case writer); 89f54a5, bcfda33,
a04f778 (ruling D20); 08a4bb5 (snapshots); 5875b79 (the Precision sentence
and the runbook row). No test suite was run: CI is green per the request,
and this sandbox has no `cargo`. What I did run: `scripts/ci/spec-drift.sh`;
a scratch script that recomputes the three new cases' values with Python
`decimal` in the 11.6 context and their hashes with `reference/formats/ids.py`;
the Python checker on three scratch copies of `int_cell_wider_than_38_digits`
(39-, 4,300- and 4,301-digit cells) and `validate.py` on two scratch steps
documents, all under `/tmp`; and fetches from GitHub of the two advisories
(rustsec/advisory-db), cargo-deny's CHANGELOG, and parquet 60.1.0's
`Cargo.toml`. **[Certain]** means I read it or ran it this session;
**[likely]** is a strong inference from code I read but did not run.

## Verdict

**Merge. Nothing blocks.** m1, m2 and m4 are closed, and so are three of the
four nits. m3 is closed for its two semantic rules: an underflow arm and a
310-digit `log` arm, each with a conformance case from the case writer. It
is **open for D17's written order**. f2055ae adds a second undefined member,
but it sits beside a bad `scope`, and both checkers report the `scope` first,
so the order of the two members is never compared. Adding that member was the
review's own suggestion, and it was wrong. Nit 4, a pointer forward from the
2026-11-18 entry, was not done. One new nit in a04f778's hunk: an `int` cell
over 4,300 digits stops the Python checker before its new width check. None of
these changes a verdict or a hash on any case or bucket that exists today.

## Findings

| Finding | Status | Evidence |
|---|---|---|
| m1 D19 case authorship | Closed | The D19 entry (`docs/decisions.md` 1413) now says the `expected.json` "was written by this session through `conformance/checker/tools/write_cases.py`, not by the independent case writer item 87 names". The rows were hand-derived in `NOTES.md` and checked against both implementations. The follow-ups entry (1419) gives the why. [Certain] |
| m2 refusal counts | Closed | `checker-diffs.md` 222: seeds 1795118323, 2 and 3; 459, 444 and 436 of 588 refused by `tm_checker::rules::validate`; 13 of the 17 defects are validation failures; no `error` or `run` row. [Certain] |
| m3 rules with no bucket | Closed for underflow and `log`; open for D17's written order | Underflow: item 89, the `overflow` arm `rng.below(6) == 5` (`gencases.rs` 2382), and the case `derive_underflow_subnormal_then_zero`. `log`: the `logexp` arm (15 %) with a 310-digit `int` literal (2074), and the case `log_of_int_beyond_double`. D17: see the section below. [Certain] |
| m4 CI presence guards | Closed differently but acceptably | 0c9f51b deletes both `[ ! -f reference/checker/refcheck.py ]` guards. The review's parenthetical also asked for the script's own check to go; it was kept. That check exits 1 with a message (`scripts/differential.sh` 38-41), so a missing file still fails the job, which is what m4 was for. [Certain] |
| Nit: P5 checkpoint row | Closed | Design 1445 states `refcheck.py <dir> --check` for each directory of both case trees, and `validate.py --check` over the three schema globs. This is what `ci.yml` 129-133 runs. [Certain] |
| Nit: README paths, 5f0eb4e | Closed | The README names `reference/formats/`, `conformance/formats/steps/` and `conformance/formats/assertions/`, and all three exist. No stale name is left under `reference/`, nor in `docs/` outside the entry that quotes them. The follow-ups entry records 5f0eb4e and the README edit as path-only. [Certain] |
| Nit: "sixteen" in D17 | Closed | `checker-diffs.md` 205 says seventeen. c08b034's subject is history and cannot change. [Certain] |
| Nit: pointer forward from the 2026-11-18 entry | Open | `docs/decisions.md` 1287 still says "sixteen named coverage buckets ... 625 cases per bucket", with no pointer. The follow-ups entry counts "four minors and three nits", but the review had four nits. Trivial. [Certain] |

## m3, the D17 part (Minor, still open): the second undefined member is never reached

f2055ae appends `aa` after `zz` in the `scope-and-undefined` defect
(`gencases.rs` 1883-1891). Item 89 now says the two members are "written out
of sorted order so D17's written order is compared". They are written in that
order: serde_json is built with `preserve_order`, and `pretty` writes in
insertion order. But the same defect sets `scope` to `"grp"`. Under 11.8's
within-op order, as D17 ruled it, `rank`'s `scope` is checked before any
undefined member. Both checkers stop at the scope:

- Rust `read_op` calls `schema::choice(m, "scope", ...)` (`steps.rs` 267)
  before `schema::undefined` (269). Its own unit test expects `/ops/0/scope`
  for `"scope":"grp","zz":1`.
- Python runs `_choice(op, "scope", ...)` (`ops.py` 189) before the
  undefined-member loop (196). On the shape the defect now writes,
  `validate.py` reports `E6002 /ops/0/scope`. With `scope` set to `all`, it
  reports `E6002 /ops/0/zz`.

So every `scope-and-undefined` case reports the `scope`, whether it carries
zero, one or two undefined members. The f2055ae host run's "no difference" is
the two sides agreeing on the `scope`. Both sides implement the written-order
rule:

- the Rust unit test `rulings_of_2026_11_20_on_the_shape_phase` gives
  `/ops/1/zz` for a `take` with `zz` and `aa`;
- Python iterates the op in file order.

Nothing compares the two sides on it, though. No bucket reaches the rule, and
the only formats case with an undefined member (`op_extra_key.json`) has just
one. The gencases comment ("the first reported is `zz`, not `aa`") and item
89's sentence both claim otherwise. [Certain for the code and the Python
probe; the Rust side is from reading, not running]

This was my suggestion in the review ("needs a second undefined member in
`scope-and-undefined`"). It was wrong for the reason above, and the builder
applied it as written.

Fix: draw the two shapes separately. One option is to split
`scope-and-undefined` per case into two variants:

- a bad `scope` with one undefined member, which reports `/ops/<i>/scope`
  (D17's scope-before-undefined order);
- a valid `scope` with `zz` then `aa`, which reports `/ops/<i>/zz` (the
  written order; the old sorted order would give `aa`).

That keeps seventeen defects and the meaning of the counts recorded under m2.
The other option is an eighteenth defect for the second variant. Either way,
item 89 changes first. The D17 Applied line (`checker-diffs.md` 205, edited
in ce3b5d3) says the bucket lets "the differential reach every ruling of this
section". That becomes true once the written order is drawn, with one
exception: a repeated `sort.by` column, which Items 3 and 7 of the review also
listed as undrawn (`repeated-name` mutates only a `select`). m3's own text did
not ask for it, so it is noted here and not counted against m3.

## New nit (a04f778's hunk): an `int` cell over 4,300 digits

`tables.py` 155 converts with `int(text)` before it measures the width.
CPython refuses to convert a decimal string longer than
`sys.int_info.default_max_str_digits` (4,300). So a 4,301-digit `int` cell
raises `ValueError`, and `refcheck.py` exits 1 with a traceback instead of
printing `E3102`. Under item 89's contract that means the case "cannot be run
at all", which is a `run` row in the differential. A 4,300-digit cell gives
`E3102`. [Certain on Python 3.11.2 here; likely the same on CI's 3.12, which
has the same limit]

Rust parses the text to a `BigInt` and refuses it at the `Decimal128(38,0)`
mapping. [likely; not run] The conversion line predates the ruling, but D20's
hunk is where it matters, since the ruling makes every `int` cell wider than
38 digits `E3102`. No case or bucket draws such a cell, so nothing differs
today. Fix: test the width on the text first, `len(text.lstrip("-")) > 38`,
then convert. `INT_RE` already refuses leading zeros, so this is the same test.

## The checks asked for

### RUSTSEC-2026-0595, and the RUSTSEC-2026-0543 ignore

- **The ignore has a reason.** `deny.toml` 28: `{ id = "RUSTSEC-2026-0595",
  reason = "parquet writes only the unencrypted parse cache (item 48) and
  never bloom filters; the vulnerable path (encrypted write with bloom
  filters) is unreachable; parquet is pinned with arrow by item 1; revisit at
  the next arrow upgrade" }`. The entry (`docs/decisions.md` 1423) makes the
  unreachability argument: the parse cache is the only Parquet write,
  `ArrowWriter` with default properties. [Certain]
- **Nothing in `crates/` writes encrypted Parquet or enables bloom filters.**
  There is one Parquet writer, `ArrowWriter::try_new(file, batch.schema(),
  None)` (`tm-files/src/parse_cache.rs` 68), and `None` means default writer
  properties. No `WriterProperties`, `bloom` or `encrypt` appears anywhere in
  `crates/`, and no DuckDB `COPY ... (FORMAT PARQUET)` either. tm-files is the
  only package that depends on parquet. [Certain]
- **A stronger argument than the entry's.** The workspace builds parquet with
  `default-features = false, features = ["arrow", "zstd"]` (`Cargo.toml` 39).
  parquet 60.1.0 gates encryption behind `encryption = ["dep:ring"]`, and
  `ring` is not in `Cargo.lock`. So the encryption code is not compiled, and
  an encrypted write cannot happen whatever the properties say. [Certain]
- **The advisory as published** (fetched today) is `informational =
  "unsound"`, category crypto-failure, patched `>= 60.2.0` (plus backports
  below 60), unaffected `< 55.0.0`. The entry's summary matches. Because it is
  informational, the CI's plain `cargo audit` (no `audit.toml` exists) treats
  it as a warning; only cargo-deny needed the ignore. [Certain for the
  advisory text; likely for cargo-audit's exit status, which I did not run]
- **RUSTSEC-2026-0543.** The ignore was removed from `deny.toml` in the same
  commit (6072a51). The advisory is still in the database: informational
  unmaintained, no patched version, not withdrawn. The entry's reason checks
  out: cargo-deny 0.22.0 (2026-11-19), PR#924, changed
  "`advisories.unmaintained` to default to `workspace` scope instead of
  `all`". chrono 0.4.45 is reached only through arrow-arith, arrow-array,
  arrow-cast and parquet, so it no longer matches. `deny.toml` does not set
  `unmaintained`, so whether chrono is reported depends on the cargo-deny
  version the action installs, as the entry says for 0.21 and earlier.
  [Certain]

### D20

- **The design sentence is in 11.7, and so in `checker.md`.** 11.7 (design
  824, `checker.md` 27, `formats.md` 98): "An `int` cell wider than 38 digits
  is `E3102` (item 47 maps `int` to `Decimal128(38,0)`); 11.6's arbitrary size
  applies to computed `int` values, which never leave Rust or Python as
  Arrow." The Domain row (`checker.md` 104) gains "(only a computed `int` can
  be: a cell is at most 38 digits)". 89f54a5 changes the design and both spec
  copies, and it precedes the code. [Certain]
- **The Python change is exactly the ruling.** After the existing `INT_RE`
  match, `tables.py` 155-159 refuses `len(str(abs(v))) > 38` with `E3102` and
  `field` None. The message is `<row, column>: '<text>' is wider than 38
  digits`. Nothing else changes, and since `INT_RE` already refuses leading
  zeros and a plus sign, the digit count is the cell's width. A 39-digit
  scratch cell gives `E3102`, `field` null, exit 0. The 4,301-digit edge is
  the nit above. [Certain]
- **The entry and the exception log exist.** `checker-diffs.md` D20 (234) has
  the case, the field, the sentences, the ruling, and an Applied line.
  `docs/decisions.md` 1446 logs the change as a ruling-specified exception to
  the no-edit rule, as for D2 and D5. [Certain]

### The three cases

- **They entered only in commits touching `conformance/checker/`.** b48e8be
  (underflow and the first log case) and 4d82af7 (the log rewrite and
  `int_cell_wider_than_38_digits`) touch only the cases, `NOTES.md`,
  `AMBIGUITIES.md` and `tools/write_cases.py`. The SQL snapshots came
  separately in 08a4bb5, which touches only `crates/tm-checker/tests/snapshots/`
  and uses the harness's own shapes (the int case's is the harness's "no SQL:
  a table did not parse (E3102)" line). [Certain]
- **No `expected.json` changed after its case writer's commit, except the D20
  rewrite.** `derive_underflow_subnormal_then_zero` and
  `int_cell_wider_than_38_digits` each have one commit.
  `log_of_int_beyond_double` has b48e8be and 4d82af7. 4d82af7's subject names
  D20, and its `NOTES.md` entry opens "Revised 11.7: an int cell is at most 38
  digits, so the huge int is computed". No other `expected.json` under
  `conformance/` changed in the range. [Certain]
- **Each `NOTES.md` entry works its value by hand from `checker.md` text it
  quotes.**
  - The underflow entry computes Etiny = -1000032, rounds `1234567E-1000035`
    to `1235E-1000032` and `1235E-1000037` to `0E-1000032`, and writes both
    forms. It then cross-checks with Python `decimal`, a library, not either
    checker.
  - The log entry computes x^9 exactly, gets 10^333 past the largest double
    and 10^306 below it, and works ln 10^306 = 306 x 2.302585092994045684 =
    704.59103845617797..., which is 704.591038456178 at 15 digits, about four
    units in the last place from the rounding boundary.
  - The int entry counts 39 digits against 11.7 and applies item 92's
    parse-first rule.

  My recomputation agrees on every value and on both hashes of the two
  non-error cases. [Certain]
- **The `Claude-Session` trailer.** The trailer on b48e8be and 4d82af7 is the
  same as on a04f778 and 89f54a5. It also appears on 38 commits back to
  2026-11-16, among them 199a1dc and every earlier case-writer commit. So it
  names the channel that committed, not the session that wrote, and is
  evidence neither way. [Certain for the count; likely for what it means]

### The Precision sentence

The sentence (11.18.2 Precision, design 1031, `checker.md` 102) reads: "An
underflowed result keeps the exponent General Decimal Arithmetic gives it, so
its scale is the negative of that exponent; a result that underflows to zero
is written at that scale." It names General Decimal Arithmetic and no
implementation or library. The specification gives an underflow that rounds
to zero the exponent Etiny = Emin - (precision - 1) = -1000032. With that
rule, the case's values follow:

- `z` = `1235E-1000032` x `1E-5`, exact `1235E-1000037`, rounds at Etiny to
  `0E-1000032`, written `0.` followed by 1,000,032 zeros.
- `y` = `1234567E-1000035` rounded at Etiny to `1235E-1000032`, written `0.`,
  then 1,000,028 zeros, then `1235`.

Both values equal the case's `expected.json` strings. The sentence came after
the case (b48e8be, then 5875b79). The entry (1452) rules that ruling 35 and
decision 3's clarification already decided the question, and no code or case
changed, so the sentence is a clarification and is recorded as one. The
runbook row (section 6, `runbook-to-p1.md` 143) defers the million-digit value
to P7, before the first `claim.checked`. [Certain]

### spec-drift

`scripts/ci/spec-drift.sh` at 5875b79 printed:

- `OK docs/spec/checker.md matches 11.6, 11.7, 11.8, 11.9, 11.10, 11.18, item 92`
- `OK docs/spec/formats.md` (11.1-11.17, 11.21, 11.23)
- `OK docs/spec/record.md` (3, 11.21)

It exits 1 here only on the `errors.md` half, which needs `TM_ERRORS_OUT`
from `cargo build -p tm-formats`. There is no `cargo` in this sandbox, so that
half is **unverified**. `docs/errors.md`, `crates/tm-formats/src/errors.rs`
and `crates/tm-formats/build.rs` are unchanged since 8d4a5a2, and the request
says the CI job is green. [Certain]
