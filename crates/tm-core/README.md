# tm-core

Toulmin's Core (design 4.1, section 9, section 10, items 72 to 78, 80, and
81): every user-level operation, all permission checks, batch assembly, the
index and its folds, the node-local stat cache, the read interface,
settings, and `tm audit`. P7 builds it without the static view (item 79,
P8); `src/ops/view_export.rs` arrives with the templates.

The Core is the only part that calls the libraries, and it passes data
between them (4.2 rule 1). Every owner operation takes an `&OwnerCap`, and
so does the main-chain append it ends in (rule 4); the crate's own tests
hold one through `tm-owner-cap`'s `test-support` feature, enabled in
`[dev-dependencies]` only.

| Module | Item | What |
| --- | --- | --- |
| `actor` | 72 | `Actor::Owner(&OwnerCap)`; `Source::detect` (`isatty`, the `harness.env_vars` list, `AGENTBOX_AGENT`) |
| `project` | 73 | `Dirs` (config and cache roots, 3.4 and OQ6), `ProjectFile`, `Binding`, `Project::open` (`E2004`, `E2005`, `E2007`) |
| `settings` | 81 | The 11.19 table (`DEFS`), `parse_value` and `validate` (`E7001`, `E7002`), `Settings` (resolved values, typed getters) |
| `batch` | 75 | `commit`: the lock's queued events first (torn tail, takeover), then the command's; `preview_ids` |
| `index` | 76 | `open`, `sync` (catch up or rebuild), `rebuild`, `apply_batch`, `read_event` (resolve by ID through `events`); `schema.sql` is 11.17's block byte for byte; `apply.rs` runs the Files and Claims folds and writes the Core's tables; `locate.rs` resolves the cache root |
| `stat_cache` | 77 | The `stat_cache` table: snapshot, confirmed fingerprint, time; `NULL` snapshot for an absent path |
| `ops` | 74 | `Core` (one command's context) and `Locked`; one file per section 9 row: `project_init`, `settings_set`, `read_in`, `file_register`, `file_hash`, `table_spec`, `file_attrs`, `file_panels`, `claim_add`, `claim_check`, `claim_review`, `claim_update_text`, `claim_retract`, `claim_supersede`, `report_add`, `report_check`, `apply`, `audit`, `lock`, `doctor`, `status` |
| `audit` | 80 | `tm audit`: verify, re-hash, re-run every check, open items, `audit/<report_hash>.json` and `.html`, `audit.completed` |
| `read` | 78 | `files`, `claims`, `reports`, `flags` (the scan-derived flags of item 66), `settings`, `audit`, `checker`; every function has a JSON rendering |

`examples/index-rebuild.rs` is `scripts/ci/bench-index.sh`'s runner.

## Tests

Every test binary of this crate links DuckDB (through `tm-checker` and
`tm-files`), so `cargo test -p tm-core` runs on the host, not in the
sandbox. The unit tests cover the actor, the project files, the settings
schema, and the index schema against the design's 11.17 block.
