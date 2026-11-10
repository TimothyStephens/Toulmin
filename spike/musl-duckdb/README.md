# musl-duckdb spike (P0, item 9)

Throwaway binary that answers D24's first question: does `duckdb-rs`
(`bundled` feature, Parquet compiled in) link statically under
`x86_64-unknown-linux-musl` with mimalloc as the global allocator and the
work on a spawned 8 MiB-stack thread, and does it print the same numbers as
the glibc builds?

It opens an in-memory database with `threads=1` and extension autoinstall
and autoload off, fills a 12-row table with `DECIMAL(18,6)` and `DOUBLE`
columns, runs three fixed queries using `ln`, `exp`, and `avg`, and prints
every result with its raw bits, followed by `time:` lines.

```sh
# glibc, dev box
cargo run --release -p musl-duckdb
# musl static (CC_/CXX_/LINKER env for the musl g++ as in ci.yml)
cargo build --release --target x86_64-unknown-linux-musl -p musl-duckdb
# without mimalloc (spike step 4)
cargo build --release --target x86_64-unknown-linux-musl -p musl-duckdb --no-default-features
# 16 MiB worker stack (spike step 4)
./target/x86_64-unknown-linux-musl/release/musl-duckdb 16
```

Two outputs agree when `diff <(grep -v '^time:' a.txt) <(grep -v '^time:' b.txt)`
is empty; ci.yml runs that over the three builds on each container.

Deleted in the change that lands `docs/decisions/musl-spike.md`.
