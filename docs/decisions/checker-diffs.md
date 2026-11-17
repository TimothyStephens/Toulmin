# Checker differences (item 93)

One entry per case where two checkers disagreed, with the sentence of `docs/spec/checker.md` that decides it and the ruling. The checkers compared so far: the case writer's `expected.json` files (written from `checker.md` alone, item 92), the P1 Rust assertion evaluator's `observed.p1.json` files, and the P4 Rust Checker (`crates/tm-checker`). An entry stays open until the owner rules; the case it names is excluded from the conformance test by name until then, and the test fails if an excluded case starts passing.

## The eval pairs: `expected.json` against `observed.p1.json` (2026-11-17)

Diffed before any P4 evaluator code was written (runbook section 6). `observed.p1.json` is `{"outcome"}` or `{"code","field"}`; it is read as verdict `outcome` with `error` null, or verdict `error` with that `{code, field}`. All 28 pairs agree, so this comparison produces no entry.

| Case | `expected.json` (case writer) | `observed.p1.json` (P1) | Agree |
| --- | --- | --- | --- |
| `absent_row` | fails | fails | yes |
| `between_reversed` | fails | fails | yes |
| `between_widened` | holds | holds | yes |
| `compare_rows_missing_right` | fails | fails | yes |
| `compare_rows_rel` | holds | holds | yes |
| `compare_rows_str_tol` | error E6004 `/tol` | E6004 `/tol` | yes |
| `count_ge` | holds | holds | yes |
| `date_between` | holds | holds | yes |
| `date_vs_datetime` | error E6004 `/expected` | E6004 `/expected` | yes |
| `eq_abs_tol_edge` | holds | holds | yes |
| `eq_exact_fails` | fails | fails | yes |
| `eq_exact_holds` | holds | holds | yes |
| `eq_rel_tol` | holds | holds | yes |
| `exists_absent` | fails | fails | yes |
| `exists_present` | holds | holds | yes |
| `ge_fails` | fails | fails | yes |
| `gt_tol_loosens` | holds | holds | yes |
| `inf_cell` | fails | fails | yes |
| `int_vs_dec` | holds | holds | yes |
| `le_edge` | holds | holds | yes |
| `lt_tol_loosens` | holds | holds | yes |
| `missing_cell` | fails | fails | yes |
| `nan_cell` | fails | fails | yes |
| `ne_with_tol_inside` | fails | fails | yes |
| `rank_absent` | fails | fails | yes |
| `rank_position` | holds | holds | yes |
| `str_eq_nfc` | holds | holds | yes |
| `str_vs_number` | error E6004 `/expected` | E6004 `/expected` | yes |

## Entries

None yet.
