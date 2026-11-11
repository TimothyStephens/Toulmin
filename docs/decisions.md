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
