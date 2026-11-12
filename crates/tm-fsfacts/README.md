# tm-fsfacts

Filesystem facts (design 4.1, D19, D25, items 26 to 31): pure and
stateless, used by the Files library, the run wrapper, and `tm doctor`.

| Module | Item | What |
| --- | --- | --- |
| `snapshot` | 26 | `Snapshot` (size, mtime, ctime, inode; no device) by `statx` with `STATX_FORCE_SYNC`, open-then-stat fallback; the 11.16 `snapshot` object with `ino` as a decimal string |
| `hash` | 27 | Streaming SHA-256 as `sha256:`, `hashing.threads`, `hashing.bandwidth_mib`, elapsed time |
| `manifest` | 28 | Directory walk to sorted entries (11.10 order), exclude patterns with the `manifest.ignore` default, the manifest fingerprint over `[{"path","fingerprint"}]`, the 11.16 `manifest` object, a directory version's `size`, and the diff (added, removed, modified) |
| `racy` | 29 | The marker file and the racy rule |
| `needs_rehash` | 30 | `Unchanged`, `HashNow`, `Unconfirmed` from two snapshots, the racy result, the settings, and the context |
| `probes` | 30a | Filesystem type by `statfs` magic, mount options, exclusive create, two-process rename visibility, locality, writability |
| `time` | | Whole-second RFC 3339 UTC with a separate nanosecond part (11.1) |

`cargo test -p tm-fsfacts` runs the unit tests and `tests/fsfacts.rs`
(item 31). The rename probe test runs `sh -c mv` as the second process.
