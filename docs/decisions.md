# Decisions and deviations log

Deviations from `docs/design/toulmin_v1_design.md`, one entry each: date,
what was done differently, why. Design-level decisions with their own record
live under `docs/decisions/`.

## 2026-11-10 P0: glibc 2.17 fallback built with zig, not in a CentOS 7 container

**What.** ci.yml and release.yml build the glibc 2.17 fallback with
`cargo zigbuild --target x86_64-unknown-linux-gnu.2.17` (glibc 2.17 headers,
zig's statically linked C++ runtime), not "inside a CentOS 7 container with
static libstdc++" as the design's Portability section words it. The CentOS 7
container build was made once, on the owner's host with podman, and its
results are in `docs/decisions/musl-spike.md`; the smoke runs on CentOS 7,
Rocky 8 and Rocky 9 were made in real containers on the host and, in the
agentbox session (no container runtime), through those distributions'
loaders and root filesystems.

**Why.** The CentOS 7 build needs an end-of-life base image and an opt-out
(`DUCKDB_CUSTOM_PLATFORM`) for an ABI DuckDB upstream refuses, and it still
links `libstdc++.so.6` dynamically at the gcc 4.8 level. The zig path needs
neither and is checked by the same symbol ceilings and the same smoke
containers. The CentOS 7 recipe is kept as the known-good alternative if the
zig job fails on CI.

## 2026-11-10 P0: local musl std built from rust-src

**What.** The local musl static spike builds used `-Zbuild-std` (via
`RUSTC_BOOTSTRAP=1` on the pinned stable 1.99.1) with the `rust-src`
component, `relocation-model=static`, and the cross toolchain's `libgcc_eh.a`
as `libunwind`. CI uses the rustup `x86_64-unknown-linux-musl` std component
and none of these flags; `.cargo/config.toml` holds only `+crt-static`.

**Why.** The sandbox proxy blocks `static.rust-lang.org`, the only source of
the prebuilt musl std; conda-forge ships Rust 1.99.1 but no musl std. The
std sources are identical, so the numbers the spike prints are the ones that
matter; the P0 checkpoint still requires the spike to pass on CI with the
rustup component.

## 2026-11-10 P0: deny.toml "allowed-crate list" is the lock file's crate names

**What.** Item 4's allowed-crate list is `[bans] allow` in `deny.toml`,
holding every crate name in `Cargo.lock` (workspace members included),
regenerated and reviewed whenever the lock changes.

**Why.** cargo-deny applies `allow` to the whole graph, so a list of the
direct dependencies alone would fail on every transitive crate. Listing the
lock's names keeps the design's intent (no crate enters unreviewed) with a
mechanical update.

## 2026-11-10 P0: symbol-versions.sh accepts CentOS 7's libstdc++ levels

**What.** For the glibc fallback the script's ceiling is GLIBC 2.17 and, for
GLIBCXX and CXXABI, the levels CentOS 7's libstdc++ 4.8.5 exports (3.4.19 and
1.3.7), rather than requiring that no libstdc++ version be referenced.

**Why.** "Static libstdc++" as the design states it is a means; the test that
matters is that the binary loads on CentOS 7. A fully static C++ runtime
references no GLIBCXX version and passes; a devtoolset-style hybrid link
would reference 3.4.19 and below and also runs there.

## 2026-11-10 P0: Cargo.lock is empty between the spike's deletion and P1

**What.** Item 2 commits `Cargo.lock`, and item 1's exact pins were locked
by building the spike. Deleting the spike (item 9) leaves a workspace with
no members, so the committed lock holds no packages until P1 adds the first
crate. The lint and deny-audit CI jobs, which cargo cannot run on a
memberless workspace, are no-ops until `crates/` exists.

**Why.** Item 9 requires the spike to be deleted in the change that lands
the decision record, and cargo cannot lock dependencies no member uses. The
pins themselves stay exact in `Cargo.toml`, so P1's first `cargo build`
locks the same versions the spike built against; `deny.toml`'s allowed-crate
list is kept as reviewed for P0 and re-checked when P1 regenerates the lock.


## 2026-11-11 P1: event bodies are schema-checked JSON, not one struct per type

**What.** Item 14 asks for "typed bodies for exactly the 11.16 gate A
types". `tm-formats` has a closed `EventType` enum with one variant per
11.16 row, and each type's body schema is a declared list of `(key, form)`
in its namespace module (`events/{project,settings,files,...}.rs`). A
`KnownEvent` is a type plus a body validated against that schema and held
as a JSON object; there is no Rust struct with a field per body key.

**Why.** Most body values have no form stated in 11.16 (titles, notes,
sizes, `sql`, `versions` members), so per-type structs would hold
`serde_json::Value` fields and add nothing a schema table does not. The
closed set, the exact key sets, the stated forms, and "an opaque event
cannot be written" all hold. The libraries that fold these events (P3,
P6, P7) can add typed accessors when they read a body.

## 2026-11-11 P1: no decimal arithmetic in typed_value.rs

**What.** Item 15 covers "the 11.6 encoding over `bigdecimal` at 34-digit
precision, half-even". `typed_value.rs` has the encoding, the lexical
rules, the 34-digit context rounding (`round_to_context`), both
IEEE-double conversions, and the Arrow mapping. It does not implement the
arithmetic operators `derive` uses (`+`, `-`, `*`, `/` with the General
Decimal Arithmetic ideal exponents) or the `E5002` overflow condition.

**Why.** Arithmetic is evaluated by the Checker (11.18.2, item 54, P4),
and where `E5002` begins is one of the facts the readiness report's B3
lists for P4's 11.18.5. Building it in P1 would decide that boundary in
code. The assertion rules item 18 needs use exact, unrounded arithmetic
(11.18.3), which `bigdecimal` gives directly.

## 2026-11-11 P1: CI gains the cross-implementation ID diff and builds the catalog for spec drift

**What.** `.github/workflows/ci.yml` (item 6) changed in two jobs. The
`conformance` job now runs the formats suite once more with
`TM_FORMATS_IDS_OUT` set, runs `python3 -I reference/formats/test_formats.py`
the same way, and diffs the two listings. The `spec-drift` job installs
the toolchain and builds `tm-formats` with `TM_ERRORS_OUT` set before
running `scripts/ci/spec-drift.sh`.

**Why.** The P1 checkpoint requires identical IDs for every case, and no
job compared the two implementations' outputs. `spec-drift.sh` (P0) fails
when `docs/errors.md` exists and `TM_ERRORS_OUT` does not point at
`build.rs` output, and the job as written in P0 never built anything.

## 2026-11-11 P1: conformance case format documented in the crate README

**What.** The shape of every file under `conformance/formats/` is
described in `crates/tm-formats/README.md`, not in a file under
`conformance/`.

**Why.** Item 90 lists only the case directories, and every crate has a
README (Detailed Implementation Plan preamble). The format is a test
harness contract between `tests/conformance.rs` and
`reference/formats/test_formats.py`, not a Toulmin format.

## 2026-11-11 P1: deny.toml allow list extended, not regenerated

**What.** P1's first build locked 106 packages. Six names not in the P0
allow list were added (`bigdecimal`, `equivalent`, `indexmap`, `tinyvec`,
`tm-formats`, `unicode-normalization`). The P0 names that the lock no
longer holds (DuckDB, mimalloc, rustix, and their dependencies, plus the
deleted `musl-duckdb`) were kept rather than dropped.

**Why.** Those crates are pinned workspace dependencies, reviewed at P0,
that P2 brings back with `tm-duckdb` and `tm-fsfacts`; regenerating from
the P1 lock would delete and re-add them a phase later. `cargo deny check`
passes (bans, licenses, sources, advisories). `musl-duckdb` should be
removed when P2 regenerates the list.

## 2026-11-11 P1: Arrow conversion errors in tm-formats use E3102

**What.** `typed_value::to_arrow` and `from_arrow` report a value that does
not fit item 47's Arrow type (scale above the column's, more than 38
digits, a timestamp outside the nanosecond range, a value of another type)
as `E3102` with no `field`.

**Why.** Item 47 names `E3102` for exactly these failures during parsing,
and item 15 places the conversion in `tm-formats`. The row and column
`E3102` carries are added by `tm-files` (P3), which knows them.

**Note, 2026-11-11 (review N9).** `to_arrow` is for parsed table values
on their way into DuckDB (item 47) and is not on the `derive` path: a
derived value that overflows is 11.6's `E5002`, decided at P4, and must
not reach `to_arrow` to surface as `E3102`.

## 2026-11-11 P1 review: clarification of pre-P1 decision 3 (negative zero)

**What.** Decision 3 in `docs/design/decisions-p1.md` says a computed
negative zero "is written `0`". 11.6 and `typed_value.rs` read that as
"written without its sign, scale kept": `-0.00` is `0.00`, not `0`. The
owner confirmed this reading on the P1 review (M6).

**Why.** Decision 3 fixes the sign, and the scale rules it adopts (the
General Decimal Arithmetic ideal exponents) fix the scale; dropping the
scale as well would make zero the one value whose scale the arithmetic
rules do not predict. No design text changed.

## 2026-11-11 P1 review: snapshot `ino` is a decimal string (B2)

**What.** 11.16's `snapshot` object carries `ino` as a decimal string
(`0|[1-9][0-9]*`) instead of a JSON integer; `size`, `mtime_ns`, and
`ctime_ns` stay integers. `events/mod.rs` checks the string form and the
`file.version_added` conformance cases carry it.

**Why.** Owner's ruling on the review: 11.1 caps JSON integers at 2^53-1
and inode numbers are unsigned 64-bit, so Lustre, GPFS, and XFS with
`inode64` would hit `E1001` at `file.register`. Recorded as a dated note
under decision 10 in `decisions-p1.md`.

## 2026-11-11 P1 review: report handle is `R<n>` (M7)

**What.** 11.16 fixes the report handle as `R<n>`, allocated by
`report.add` like `F<n>`; `report.registered` carries `report` and
`path`; `claim.added.report` is an `R<n>`; 11.20's report page is
`reports/R<n>.html`. The "format not fixed in gate A" sentence is gone,
`ReportHandle` parses `R<n>`, and the `report.registered` and
`claim.added` conformance cases carry `R1`.

**Why.** Owner's ruling on the review. Disagreement recorded: the design
already uses `R<n>` for run handles (the glossary's `R12`, 11.16's
`origin.run`, `tm run again R10`, the CUJs). The field a handle sits in
tells the two apart in the record, but the glossary presents handles as
one namespace the CLI and the view pages address by handle alone. Applied
as ruled; the collision is reported for a one-letter decision before P6
builds `report.add`.

**Ruling, 2026-11-11.** The owner changed the report handle to `T<n>`;
`R<n>` stays the run handle. 11.16, section 9's `report.add` row, 11.20's
page path (`reports/T<n>.html`), `ReportHandle`, and the `report.registered`
and `claim.added` cases now carry `T<n>`.

## 2026-11-11 P1 review: assertion cases split into schema and eval (M5)

**What.** `conformance/formats/assertions/` holds only
`schema/*.json` (11.9 validity and refusal). The evaluation cases, which
name an `observed` value and an 11.18.3 outcome or error, moved unchanged
to `conformance/checker/assertions/eval/`; `tests/conformance.rs` in
`tm-formats` no longer reads them, and no test runs them until
`tm-checker`'s conformance test (item 59) at P4. Items 59, 90, and 92, the
P5 row, and the two Verification Plan conformance rows say so.

**Why.** Owner's ruling on the review: an assertion's outcome is a
Checker fact, and P1 does not freeze 11.18.3. The `evaluate` function and
its unit tests stay in `assertion.rs` (item 18), unchanged.

**Layout, 2026-11-11 (owner).** Each eval case is a directory
`eval/<name>/` with `case.json` (the assertion and `observed`) and
`observed.p1.json` (the outcome or error the P1 Rust side gave). P4's
independent session writes `expected.json` from `checker.md` and
`case.json` only, so it cannot mistake P1's outcome for the expectation;
`observed.p1.json` is kept only for comparison. Item 92 states this.

## 2026-11-11 P1 review: integer range is plus or minus 2^53-1 (N1)

**What.** 11.1, `canonical.rs`, and `canonical_json.py` accept integers
within plus or minus 2^53-1 and refuse 2^53 itself with `E1001`. The
`int_limits` canonical case and the RFC 8785 test changed accordingly.

**Why.** RFC 7493 (I-JSON) gives `[-(2^53)+1, 2^53-1]` as the range every
implementation represents exactly; the earlier text, inclusive of 2^53,
did not match the standard 11.1 cites.

## 2026-11-11 P1 review: a non-NFC `str` is normalized, not refused (N3)

**What.** 11.6 says a `str` `v` that is not NFC is replaced by its NFC
form when read, never refused. `TypedValue::from_json` normalizes instead
of raising `E1004`; the `str_not_nfc` typed-value case now expects the
normalized value.

**Why.** Owner's ruling on the review: 11.6's table said "NFC-normalized"
and its lexical-form paragraph said "must already be NFC", and the two
disagreed on whether a reader refuses. Normalizing matches the table and
11.22's drift normalization, which also normalizes rather than refuses.

## 2026-11-11 P1 review: formats codes on the section 9 rows (N4)

**What.** `claim.add` (and `claim.supersede`, which reads "as
`claim.add`") lists `E1001`, `E1003`, and `E1004` for its `--steps` and
`--assert` files; `apply` lists `E1001` and `E1003` for the action file.
No other gate A row reads a JSON document the user supplies.

**Why.** Owner's ruling on the review: section 9 names the codes each
operation can produce, and these three were produced only by rows that
did not list them.

## 2026-11-11 P1 review: a higher nested format version makes the event opaque (M1)

**What.** 11.16 says a `steps.steps`, op `v`, `assertion.assert`, or
`spec.spec` above the reader's highest known version makes the whole
event opaque under 11.11, like a higher `tv`; the body is not checked.
`E6002` for an unknown version stays for author input. `envelope.rs`
checks this before the body schema, and two envelope cases (`claim.added`
with `filter` v2, `file.parse_spec_set` with `spec` 2) read as opaque.

**Why.** Reviewer's fix. Without it a later op version would turn a valid
later chain into `E1002` at the line level for a gate A reader, defeating
per-op versioning.

## 2026-11-11 P1 review: both ref spellings are one form; `kind` is `input` or `output` (M2)

**What.** `ClaimRef` parses `C<n>` and `S<n>.<m>`; 11.16 says the ref form
is the same in every gate and gate A operations allocate only `C<n>`.
`file.registered.kind` is `input` or `output`. 11.22's span regex now
follows the no-leading-zero rule. The two invalid cases that froze the
refusals (`claim.retracted` with `S4.12`, `file.registered` with
`output`) are replaced by other refusals, and valid cases carry each.

**Why.** Reviewer's fix: the glossary, 7.1, 5.4, 11.22, and 6.1 already
give promoted session claims and output files main-chain events at tv 1,
so refusing them in the body form would cost gate B and C a tv bump on
seven types.

## 2026-11-11 P1 review: `src` only for owner actors; reserved `src.confirmed_by` (M3)

**What.** 11.2: `src` is `null` unless `actor.kind` is `owner`, and then
carries the three D23 facts plus the reserved `confirmed_by` (an event
ID) when present. 11.13 lists `src.confirmed_by`. `Head.src` is an
`Option`; the journal-line case carries `src: null`, and new cases cover
a session actor with `src`, an owner without it, and `confirmed_by`.
I chose `null` rather than "absent or null" so one fact has one encoding.

**Why.** Reviewer's fix: 11.2 says `src` is for owner actions, and D27
needs somewhere to put the confirming viewer event's hash without an
`env` or `tv` bump.

## 2026-11-11 P1 review: a form for every body value (M4)

**What.** 11.16 states a form for every key; the one `Any` left is
`claim.checked.assertion.observed`, decided at P4 (item 18, B3). Choices
made here: `content_label` is `content_changed`, `carried_forward`, or
`metadata_only` (snake_case like `origin.kind`); `size` and
`compressed_size` are integers, a directory version's meaning being item
28's; `algorithm` is `md5` or `sha256`; `title`, `description`,
`sentence`, `reason`, and `description` in `claim.checked.steps` are
non-empty strings; `source`, `qualifiers`, `note`, and `evidence.detail`
are strings or `null`; `key_map` is a list of lists of typed values;
`parse_specs` maps version handles to IDs. `events/*.rs` checks each.

**Why.** Reviewer's fix: `Any` in a hashed body lets one fact take two
encodings and two IDs, and tightening later at the same `tv` would break
frozen valid lines. The reviewer's table showed each "open" form was in
fact stated elsewhere in the design.

## 2026-11-11 P1 review: assertions are checked against the table schema (M5, second part)

**What.** 11.9 states a validation-time check of an assertion against the
final table's schema (`row` arity and types, `column` existence,
`expected` and `compare_rows` type compatibility) with the codes and
pointers; `Assertion::check(&TableSchema)` implements it, and
`assertions/schema/` cases with a `table` member exercise it. 11.9 also
says a mismatch found at a re-check is verdict `error` with `E6004`.

**Why.** Reviewer's fix: the type mismatches the moved eval cases raised
at evaluation time are catchable before anything runs, since
`TableSchema` exists, and 7.3 says re-checks record rather than refuse.

## 2026-11-11 P1 review: the formats row says what Python checks when (M8)

**What.** The Verification Plan's "Conformance: formats" row now says
Python checks the meaning of `canonical/`, `ids/`, and the line level of
`envelope/` at P1, and of `steps/` and `assertions/schema/` from P5; the
rest of the suite is a hash agreement at P1.

**Why.** Reviewer's finding: the row claimed "by both" for validation
results the Python side does not compute until item 87's `validate.py`.

## 2026-11-11 P1 review: nits taken (N7, N8, N10) and declined (N2, N5, N6, N11)

**What.** N7: the Spec drift row states the normalization rule itself and
names the script as its implementation. N8: `conformance/formats/` gains
`parse_spec/` and `citations/` (JSON form and CLI spelling), so `E3103`,
`E3104`, `E3105`, and `E6007` have cases; item 90 lists them. N10: the
11.18.2 `rank` row says a table with a `rank` column must `select` it
away first.

Declined: N2 (fault precedence), because "either code is conforming"
would make the `float_before_duplicate` case and the Python check
ambiguous, and the rule is stated in 11.1, not left to an implementation;
N5 (a separate code for schema faults), because it adds a code the
catalog gates and section 9 would have to carry for a readability gain G2
does not require, and `field` already locates each fault; N6 needs no
change (noted: `E6006` is taken); N11, because changing ID derivation is
a `record.json` `hash_alg` change, not an envelope version change, and a
line's ID must be checkable to chain `prev`.
