# Decisions and deviations log

Deviations from `docs/design/toulmin_v1_design.md`, one entry each: date,
what was done differently, why. Design-level decisions with their own record
live under `docs/decisions/`.

## 2026-11-10 P0: spike builds and smoke runs made without containers

**What.** The P0 spike's "runs on CentOS 7, Rocky 8, Rocky 9" check was done by
running each binary through those distributions' own dynamic loader and
libraries (root filesystems from the CentOS and Rocky image repositories on
GitHub), not inside containers. The "glibc inside a CentOS 7 container with
static libstdc++" build was not made; a glibc 2.17 build cross-compiled with
zig's glibc headers and a statically linked libc++ stands in for it, and that
is also the fallback build ci.yml and release.yml produce.

**Why.** The agentbox session that did P0 has no container runtime (no podman
socket, chroot refused, no container registry reachable through the proxy).
The loader-based run exercises the same old glibc and libstdc++ a container
would, so it answers the fallback question; it does not exercise an old
kernel, which neither would. The zig path was the only glibc 2.17 build that
could be verified end to end from this environment, and it needs no CentOS 7
image or devtoolset repository, both of which are end-of-life. Details and
numbers: `docs/decisions/musl-spike.md`.

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
