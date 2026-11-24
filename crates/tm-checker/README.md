# tm-checker

Toulmin's Checker library (design 4.1, 11.18, items 53 to 62): query
steps and an assertion over cited tables, evaluated to a verdict. A
library in the sense of 4.2: it calls no other library. The Core hands it
the cited tables as Arrow with their parse specs (`InputTable`), the steps,
the assertion, the claim's citations, the engine settings, and a row limit;
it returns a `RunResult`.

| Module | Item | What |
| --- | --- | --- |
| `eval` | 53 | `filter.where` and `derive.expr` over the rows, `bigdecimal` under the 11.6 context, decimal128 (precision 34, `Emax` 6144, `Emin` -6143, half-even): exact arithmetic, then `fix`, the General Decimal Arithmetic rounding and range step; `E5002` on Overflow; the `log` family through IEEE double, 15 significant digits |
| `translate` | 53 | One explicit `DuckDB` statement per relational op (`sort`, `take`, `rank`, `group`, `join`, `select`), binary collation, `NULLS FIRST`/`LAST`, key tie-breaker, the `ties` mapping, group order, `member_count`, `member_keys_hash` |
| `table` | 53 | The working table (typed values, columns, key) and its encoding for `DuckDB`: text as itself, `bool` as `BOOLEAN`, every other value as an order key whose binary order and equality are the 11.18.2 ones, `missing`/`nan`/`inf` as `NULL` |
| `engine` | 54 | One `tm-duckdb` connection per check, tables registered by name, version and extension facts |
| `run` | 55 | The check: validate, step 0 (the start table in key order), each op, the hashes, the assertion; the result type with every step's capped table, the emphasized rows and columns, and the named rows beyond the cap |
| `describe` | 56 | The 11.18.4 sentences |
| `rules` | 57 | The checks before anything runs, in order; `RULES_VERSION` |
| `verdict` | 58 | `Verdict`, the Checker's members of the `claim.checked` body |

Why cells reach `DuckDB` as order keys rather than `DECIMAL(38, s)`: a
derived `dec` column holds values of different scales (`10 / 4` is `2.5`
and `4 / 4` is `1`), and an `int` is of arbitrary size, so no one decimal
column type holds them. `DuckDB` orders, ranks, groups, and joins by the
keys and returns row numbers; the typed values never leave Rust. See
`docs/decisions.md` (2026-11-17, P4).

## Tests

- `tests/conformance.rs` (item 59): every case under
  `conformance/checker/cases/` and `conformance/checker/assertions/eval/`
  against its `expected.json`; a case with an open entry in
  `docs/decisions/checker-diffs.md` is listed in `OPEN`.
- `tests/property.rs` (item 60): five properties, 1,000 cases each.
- `tests/snapshot_sql.rs` (item 61): the generated SQL per conformance
  case, `cargo insta test -p tm-checker`.
- `examples/refcheck.rs` (item 59): `cargo run -p tm-checker --example
  refcheck -- <case-dir>` prints the case's `expected.json`-shaped object
  on standard output, the contract it shares with `refcheck.py` (item 89);
  it never writes `expected.json`.
- `examples/gencases.rs` (item 89): `cargo run -p tm-checker --example
  gencases -- --out DIR --n N [--seed SEED] [--start I]` writes N
  differential cases in item 92's layout, one `gen.json` each with the
  seed, index, and bucket; `scripts/differential.sh N [SEED]` runs both
  refcheck implementations over them and tabulates every differing field.
- `fuzz/` (item 62): `steps_json`, `derive_expr`, `csv_parse`, run weekly
  by CI with `cargo +nightly fuzz run <target> -- -max_total_time=600`
  from this directory.
