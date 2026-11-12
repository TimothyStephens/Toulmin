# tm-files

Toulmin's Files library (design 4.1, section 6, items 44 to 52): file
identity, versions, attributes, parse specs and keys, the parse cache,
PDF page counts, image panels, provider checksums, and the Files fold.
A library in the sense of 4.2: it calls no other library, takes
already-authorized decisions from the Core, and returns events
(`KnownEvent`s) for the Core to batch; it never writes the record.

| Module | Item | What |
| --- | --- | --- |
| `identity` | 44 | `Identity` (input path, title, description, source, level, retired, removable); `check_input_path` (`E3001`); `register` (`E3002`), `rename`, `mark_removable`, `restore`, each with its event |
| `attributes` | 46 | `State` and `Action`: the 6.1 to 6.3 transitions behind `E3201` (`E3005` for `hash` without `--again`, `E3301` for panels on a non-image) |
| `version` | 45 | `Candidate` (what `tm-fsfacts` found, with the decompressed fingerprint of a `.gz` or `.bgz` file), `Version`, `Origin`, `ContentLabel`, the three equivalences (`same_table_by_key`, `decompressed_fingerprint`), `Version::next` (a same-fingerprint candidate adds nothing; a byte-different one is labeled by the equivalence; the event is `file.version_added` at `tv` 2) and `Version::kept` with their events |
| `parse` | 47 | `complete_spec` (`tm table spec`: scales from the file), `parse` (CSV and TSV through DuckDB's `read_csv` with every guess off, Parquet through its Arrow reader, cells typed under the spec, item 47's Arrow types), `typed_rows`, `key_tuples`; `E3101`, `E3102`, `E3103` |
| `parse_cache` | 48 | `ParseCache`: `<cache>/<file fingerprint>/<spec fingerprint>.parquet` |
| `pdf` | 49 | `page_count` through `lopdf`; `E3302` |
| `image` | 49 | `is_image` by signature; `set_panels` (`file.panels_set`) |
| `checksum` | 50 | GNU and BSD checksum lines (`E3006`); `compare` against Files' fingerprints, or the entries' MD5 for an MD5 list; `file.checksum_checked`; the `E3004` refusal |
| `fold` | 51 | `fold(&mut IndexTx, &Line)` into `files`, `file_versions` (with `decompressed_fingerprint`), `parse_specs`, `panels`; a line is validated at its own `tv` |

`decompressed_fingerprint` reads `.gz` and `.bgz` files through
`flate2`'s multi-member decoder (`.bgz` is concatenated gzip members) and
hashes the decompressed bytes; `checksum::compare` hashes the entries'
bytes with MD5 for an MD5 list (item 1: MD5 is used only there). A page
citation on a file without a PDF page tree is `E3302`; a checksum list in
neither form, or with mixed algorithms, is `E3006`.

## Tests

`cargo test -p tm-files` runs the unit tests and `tests/files.rs` over the
workspace `fixtures/` (item 82a): registration, a spec with each type,
a duplicate key, each equivalence rule, each 6.3 transition, both checksum
forms, page counts and panels, the parse cache, and the fold. The same
test checks that the fixture's specs, steps, assertions, and citations
are well formed under `tm-formats`. Parsing needs the bundled DuckDB, so
the first build is long.
