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

