# P0 musl spike: outcome and chosen build path

Date: 2026-11-10. Design items 9 and 10, decision D24.

## Decision

`tm` ships as a **static `x86_64-unknown-linux-musl` binary, DuckDB compiled in
(`duckdb-rs` `bundled`), mimalloc as the global allocator, real work on a
spawned thread with an 8 MiB stack**. None of the step 4 fallbacks (no
mimalloc, 16 MiB stack, prebuilt libduckdb) was needed. The glibc 2.17
fallback was built both ways: inside a CentOS 7 container with devtoolset
(the design's wording) and with `cargo zigbuild --target
x86_64-unknown-linux-gnu.2.17`. CI and release use the zig path; see
"Fallback build" below and `docs/decisions.md`.

## What the spike does

`spike/musl-duckdb` opens an in-memory DuckDB with `threads = 1`,
`autoinstall_known_extensions = false`, `autoload_known_extensions = false`,
creates a 12-row table `t (id INTEGER, d DECIMAL(18,6), x DOUBLE)` with fixed
values spanning 1.23e-4 to 1.2e15, and runs three fixed queries: per-row
`ln(d)`, `exp(d/100)`, `ln(x)`, `exp(x/1e14)` and a mixed expression; one
aggregate row (`avg` over DECIMAL as text and as DOUBLE, `avg` over DOUBLE,
`avg(ln(..))`, `avg(exp(..))`, a geometric mean, `ln(avg(d))`, `sum(d)`,
`stddev_samp(x)`); and the same aggregates grouped by `id % 3`. Every DOUBLE
is printed as Rust `{:?}` (shortest round-trip repr) plus its IEEE-754 bit
pattern, so "identical numbers" means identical bits. Output lines `time:`,
`build:` and `worker_stack_mib:` are stripped before comparison.

DuckDB `v2.0.0` (`duckdb` crate `1.20000.1`); loaded extensions after open:
`core_functions, parquet`. The bundled build compiles with
`DUCKDB_EXTENSION_AUTOINSTALL_DEFAULT=1` and `AUTOLOAD_DEFAULT=1`, so the
spike sets both to `false` through `Config` and again with `SET`; the printed
settings confirm `false`.

## Results

| Build | Toolchain | Size (bytes) | Linkage | Dev box (glibc 2.43) | CentOS 7 | Rocky 8 | Rocky 9 | Numbers |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| musl static, mimalloc, 8 MiB (host, rustup 1.99.1 musl std, musl-cross gcc 16.2) | CI path | 68,446,232 | static: no interpreter, no NEEDED | runs | runs (podman `centos:7`) | runs (podman `rockylinux:8`) | runs (podman `rockylinux:9`) | identical |
| musl static, mimalloc, 8 MiB (sandbox, `-Zbuild-std` std from rust-src, same gcc) | local only | 66,846,552 | static | runs | runs (podman); the loader method cannot host a static binary (see below) | runs (podman and Rocky 8 loader) | runs (podman and Rocky 9 loader) | identical |
| glibc dynamic, dev box (Ubuntu 26.04, gcc 14, glibc 2.43) | `cargo build --release` | 65,327,568 | needs GLIBC_2.34, GLIBCXX_3.4.30, CXXABI_1.3.13 | runs | fails to load (GLIBC_2.18, 2.25, CXXABI_1.3.13 missing) | fails to load (GLIBC_2.30, 2.32, CXXABI_1.3.13 missing) | fails to load (GLIBCXX_3.4.30 missing) | identical |
| glibc inside a CentOS 7 container (host podman, `manylinux2014` = CentOS 7.9 + devtoolset-10, `-static-libstdc++ -static-libgcc`) | design's fallback as written | 64,938,568 | dynamic: interpreter `/lib64/ld-linux-x86-64.so.2`; needs GLIBC ≤ 2.17, GLIBCXX ≤ 3.4.19, CXXABI ≤ 1.3.7 (`libstdc++.so.6` still needed at the gcc 4.8 level: devtoolset's `-static-libstdc++` links only its non-shared part) | not run | runs (podman) | runs (podman) | runs (podman) | identical |
| glibc 2.17 fallback (zig 0.17.1, cargo-zigbuild 0.24.1, static C++ runtime, `-g0`) | `cargo zigbuild --target x86_64-unknown-linux-gnu.2.17` | 46,510,480 | dynamic: needs GLIBC ≤ 2.17 only; no libstdc++, no libgcc_s | runs | runs (podman `centos:7`, exit 0); through the loader method it aborts at exit, see below | runs (podman and loader) | runs (podman and loader) | identical |
| same, without mimalloc (`--no-default-features`) | bisect of the loader abort | 46,375,352 | as above | runs | runs (podman); same abort under the loader | runs | runs | identical |

"Identical" means the stripped outputs are byte-identical (sha256 prefix
`d5e75d22aaad59b8`) across every build and machine that ran. Representative
values from the aggregate row, all builds:

```
avg_d      = 113.95926874999999   [0x405c7d64a8c154c9]   (DECIMAL avg as text: 113.95926874999999)
avg_ln_x   = -55.825250474443216  [0xc04be9a1cebb5efd]
avg_exp_d  = 19164.771101136783   [0x40d2b73159b89519]
geomean_x  = 5.693794231395648e-25 [0x3ae606dc55c29f35]
ln(avg_d)  = 4.735841092883954    [0x4012f18053d3f53a]
sum_d      = 1367.511225           (DECIMAL, exact)
```

Wall time (open + three queries, single run, i7-3930K):

| Build | open_ms | query_ms | total_wall_ms |
| --- | --- | --- | --- |
| musl static, host rustup build | 64 | 30 | 124 |
| musl static, sandbox build | 65-87 | 29-41 | 124-164 |
| glibc dynamic, dev box | 59 | 25 | 106 |
| CentOS 7 container build, run in `centos:7` / `rockylinux:8` / `rockylinux:9` | | | 72 / 67 / 67 |
| musl static (host build), run in the same three containers | | | 1548 / 103 / 108 |

Container timings are single cold runs of a 65-68 MB binary from a bind
mount; the 1.5 s and 2.1 s first runs on `centos:7` are page-in, not DuckDB.
The CentOS 7 build's own run inside the build container, under compile load,
reported `open_ms=42 query_ms=366`.

Full outputs, symbol reports and binaries are under `.local-tools/out/`
(not committed; the CI artifacts `musl` and `glibc217` carry the same files).

### Test method for the old distributions

Two methods were used. On the host, rootless podman ran every binary in
`centos:7` (7.9.2009), `rockylinux:8` (8.10) and `rockylinux:9` (9.8); those
are the results in the table and the same thing ci.yml's smoke job does. In
the agentbox session, which has no container runtime, each binary was also
run through the distribution's own `ld-linux-x86-64.so.2` and libraries from
the official root filesystems. That is a valid test for dynamic binaries but
not for static ones: glibc 2.17's `ld.so` segfaults when asked to run any
static executable (a three-line C hello too), while 2.28 and 2.34 run it. The
loader runs agreed with the container runs wherever both applied.

## Issues met and what they mean for the build path

1. **`libduckdb-sys` default features pull an HTTP client.** `download-lib`
   is on by default and brings `ureq`; item 1 therefore pins `duckdb` with
   `default-features = false` and features `bundled`, `parquet`, `vtab-arrow`.
   `deny.toml` bans `ureq` so this cannot regress.
2. **mimalloc under musl needs libgcc's `__popcountdi2`.** mimalloc's C is
   compiled for baseline x86-64 (no POPCNT), so gcc emits a call to
   `__popcountdi2`. The rustup musl std satisfies it from `compiler_builtins`
   (built with its `c` feature, compiler-rt sources); the host build linked
   without any extra flag. Only the sandbox's std built from `rust-src` lacked
   it. No change to item 1 or `.cargo/config.toml`.
3. **Compiling bundled DuckDB needs memory.** Under a 4 GiB cgroup, g++ at
   `-O3` on DuckDB's unity files was OOM-killed at five parallel jobs (33
   kills) and still at two (4 kills, `ub_src_main.cpp`,
   `ub_src_parser_peg_transformer.cpp`); two jobs plus
   `--param ggc-min-expand=20 --param ggc-min-heapsize=65536` completed. The
   host with 9 GiB and four jobs completed without tuning. A cold DuckDB
   compile took 53 minutes in the sandbox at two jobs and is the dominant CI
   cost; `Swatinem/rust-cache` keeps the `libduckdb-sys` build output between
   runs.
4. **Stack.** 8 MiB on the worker thread was enough for every query; the
   16 MiB variant was not needed.
5. **cargo-zigbuild compiles C++ with `-g`.** Its compiler wrapper passes
   `-g` to every C and C++ compile. At `-O3 -g`, clang on
   `ub_src_parser_peg.cpp` was OOM-killed under the 4 GiB cgroup after 372
   other objects had compiled (81 minutes lost). The release binary carries no
   debug info, so the glibc fallback jobs in ci.yml and release.yml set
   `CFLAGS_x86_64_unknown_linux_gnu` and `CXXFLAGS_x86_64_unknown_linux_gnu`
   to `-g0`, which comes after the wrapper's `-g` and wins.
6. **DuckDB refuses CentOS 7's C++ ABI.** Building inside a CentOS 7
   container with devtoolset (the design's fallback as written) stops in
   `platform.hpp` with `#error "DuckDB does not provide extensions for this
   (legacy) CXX ABI"`, because devtoolset compiles against the pre-C++11
   libstdc++ ABI (`_GLIBCXX_USE_CXX11_ABI=0`). It builds only with
   `-DDUCKDB_CUSTOM_PLATFORM=linux_amd64_gcc4`, which upstream treats as
   unsupported for extensions. With extensions compiled in and downloads off
   this is harmless, but it means the CentOS 7 build is a configuration
   DuckDB itself has dropped. The zig build uses the current ABI and does not
   need it.

## Chosen path: what changes in item 1 and `.cargo/config.toml`

Item 1 (workspace `Cargo.toml`), as now written:

- `duckdb = { version = "=1.20000.1", default-features = false, features = ["bundled", "parquet", "vtab-arrow"] }`.
- `mimalloc = { version = "=0.1.52", default-features = false }`, confirmed.
- `[profile.release] lto = "fat", codegen-units = 1, panic = "abort"`.

`.cargo/config.toml`, as now written:

- `[target.'cfg(all())'] rustflags = ["-D", "warnings"]`.
- `[target.x86_64-unknown-linux-musl] rustflags = ["-D", "warnings", "-C", "target-feature=+crt-static"]`.
- Nothing else. The musl C/C++ compiler, archiver and linker come from the
  environment (`CC_x86_64_unknown_linux_musl`, `CXX_…`, `AR_…`,
  `CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER`), set by ci.yml and
  release.yml to the pinned `cross-tools/musl-cross` 20261001 toolchain
  (sha256 `faeaf3c7c377cb4351490988d9a1c8fed7d403a5b8066e5103af5d4dd96450fe`).
  Putting a linker path in the config file would break every machine whose
  toolchain lives elsewhere.
- Not needed, and not added: `-C relocation-model=static`,
  `-C link-self-contained=no`, any `-L native`, a 16 MiB stack, a different
  allocator.

Item 5a stands: the real entry point runs on a spawned thread with an 8 MiB
stack.

## Fallback build

The design's fallback, "glibc 2.17 with static libstdc++", was built as
written on the host: `quay.io/pypa/manylinux2014_x86_64` (CentOS 7.9,
devtoolset-10, gcc 10), rustup 1.99.1, `RUSTFLAGS="-D warnings -C
link-arg=-static-libstdc++ -C link-arg=-static-libgcc"`, plus
`CXXFLAGS=-DDUCKDB_CUSTOM_PLATFORM=linux_amd64_gcc4` (issue 6). Findings:

- It needs `libstdc++.so.6` and `libgcc_s.so.1` at run time after all:
  devtoolset's `-static-libstdc++` links its non-shared supplement
  statically and leaves the gcc 4.8-level shared library as a dependency.
  The versions it needs (GLIBCXX 3.4.19, CXXABI 1.3.7) are exactly what
  CentOS 7 ships, so it loads on CentOS 7, Rocky 8 and Rocky 9; those are
  the ceilings `scripts/ci/symbol-versions.sh --max-glibc 2.17` enforces.
- Its numbers are bit-identical to the musl binary's on every distribution,
  including on CentOS 7 where `ln`/`exp` come from glibc 2.17's `libm`.
- It needs an EOL base image and an opt-out DuckDB upstream does not
  support (issue 6).

The zig alternative (`cargo zigbuild --target x86_64-unknown-linux-gnu.2.17`,
zig's static libc++, no CentOS 7 image, no ABI opt-out) was built in the
sandbox after the memory fix (issue 5): 46.5 MB, needs only GLIBC ≤ 2.17,
same numbers everywhere, and it exits cleanly in the real `centos:7`,
`rockylinux:8` and `rockylinux:9` containers (host podman) and on the CI
runner. One caveat about test methods: when this PIE binary is started by
invoking CentOS 7's `ld-linux-x86-64.so.2` explicitly with `--library-path`
(the sandbox's loader method), it prints everything and then aborts inside
glibc 2.17's `free` during process exit (exit 134), with or without
mimalloc; the same invocation on glibc 2.28 and 2.34 exits cleanly, and the
CentOS 7 devtoolset build (non-PIE) is unaffected. The abort does not occur
when the kernel starts the binary normally, so it is an artefact of the
explicit-loader method on glibc 2.17 and not a property of the shipped
binary. Consequence: the zig build is the fallback ci.yml and release.yml
produce; the smoke job in real containers, not the loader method, is the
check that counts for it.

## What this contradicts or qualifies in the design

- **Portability, "glibc 2.17 with static libstdc++".** The fallback that CI
  and release build is a zig cross build against glibc 2.17 headers with a
  statically linked C++ runtime, not a CentOS 7 devtoolset build. The
  binary's symbol requirements are what the design asks for; the toolchain
  differs. The CentOS 7 container build is recorded above when the host run
  completes.
- **Determinism, "cross-machine verdicts must match".** The static musl binary
  carries musl's `libm`, so `ln` and `exp` are the same code on every machine.
  Any glibc build takes `ln`/`exp` from the host's `libm.so` at run time.
  glibc 2.28 replaced `exp` and `log` with the ARM optimized-routines
  implementations, which musl also adopted, so musl and glibc 2.28+ agreeing
  bit for bit (as they did here against glibc 2.43) is expected. glibc 2.17
  on CentOS 7 still has the older IBM correctly-rounded implementations; the
  newer ones are not correctly rounded in every case, so the fallback binary
  running on CentOS 7 can in rare inputs differ in the last bit from the musl
  binary. The spike's inputs did not hit such a case: the CentOS 7 build on
  glibc 2.17's `libm` matched the musl binary bit for bit. The design's
  "verdicts must match" therefore holds for the musl binary everywhere and
  for the fallback on glibc 2.28+, and is only probable, not guaranteed, for
  the fallback on glibc 2.17. Consequence for later phases: the verdict fingerprint should
  record the build flavour (musl or glibc) next to the DuckDB version, and
  the fallback should be documented as "runs, verdicts may differ in the last
  bit of `log`/`exp`-derived values on glibc older than 2.28". Not a P0
  change.
- **Portability, old kernels.** Neither the container smoke tests nor the
  loader runs exercise an old kernel; CentOS 7's 3.10 kernel is only reached
  if a smoke host actually runs it. Not a P0 question.

## Status against the P0 checkpoint

- Spike passes locally three ways with identical numbers: done.
- Spike passes on the three target distributions in real containers (host
  podman): done for the musl binary, the zig fallback and the CentOS 7
  container build.
- Spike on CI: the first run (commit `11e29ba`) built and ran it under musl,
  the zig glibc 2.17 build and the runner's glibc, with numbers identical to
  the musl reference; its smoke jobs failed on an artifact-path bug in the
  workflow (fixed in `3b8fcee`). This change, which deletes `spike/`, is
  pushed only after the run on `3b8fcee` is green.
- With the spike gone the workspace has no members until P1. The CI jobs
  that need a crate (format and Clippy, cargo deny and audit, the musl,
  glibc 2.17 and smoke jobs) are no-ops until their crate exists, and
  `Cargo.lock` holds no packages until P1 adds the first crate; see
  `docs/decisions.md`.
- The CentOS 7 container recipe mentioned under "Fallback build" was run from
  a local, uncommitted script; its exact flags are the ones listed there.
