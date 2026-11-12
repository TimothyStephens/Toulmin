# tm-duckdb

The DuckDB engine crate (design 4.1, item 26a, D24): `open(EngineSettings)
-> Engine`, `Engine::register_arrow(name, batch)`, `Engine::query(sql) ->
RecordBatch`, `Engine::facts() -> {version, extensions, build}`. The
`duckdb` dependency lives only here, and the bundled binding is the only
`unsafe` in the workspace.

Settings applied on open, each both in the `Config` and by a `SET`
afterwards (the bundled build defaults autoinstall and autoload to on,
P0): `threads = 1`, `memory_limit` from `checker.memory_limit_mib`,
`temp_directory` from `checker.temp_directory`,
`autoinstall_known_extensions = false`, `autoload_known_extensions =
false`. `query` interrupts after `checker.call_seconds` and reports
`Error::Timeout`. `effective_settings` reads the five back for the NFR
Budgets snapshot. `facts().build` is `musl` or `glibc` from the target.

`register_arrow` copies the batch into a DuckDB table of the given name
through the `arrow` table function, so queries never depend on Arrow
buffers outliving the registration.
