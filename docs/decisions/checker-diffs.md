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

## The differential runs: the P4 Rust Checker against the Python reference checker (2026-11-19)

`scripts/differential.sh 10000` twice (item 89), the Python side `reference/checker/refcheck.py` as committed in 199a1dc and 5f0eb4e, the Rust side `crates/tm-checker` at cc0e202. A case is reproduced by `gencases --seed SEED --start INDEX --n 1`; every differing case is kept under `target/differential/<seed>/<case>/` with both outputs. These are generated cases, not conformance cases, so no `OPEN` exclusion follows from an entry; the entry stays open until the owner rules.

| Run | Seed | Cases | Differing cases | Differing fields | Distinct causes | Both sides ran | Wall clock |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | 1795118323 (Unix time) | 10,000 | 3,405 | 4,960 | 5 (D1 to D5) | 4,273 s | 72 min 31 s |
| 2 | 2 | 10,000 | 3,328 | 4,803 | 3 (D1 to D3) | 4,383 s | 74 min 17 s |

Cause D1 is a defect of the case generator, not of either checker: the Python side never ran on 21 % of the cases. To compare those cases too, the Python side was re-run on them through a shim outside the repository that copies the case with its `.tsv` files named `.csv`; the shim's results are the `python-shim.*` files beside each kept case and are counted under D2 and D4 below. The comparison of every other case is the script's own.

| Cause | Run 1 cases | Run 2 cases | Who is wrong under `checker.md` |
| --- | --- | --- | --- |
| D1 generator names tab-separated files `.tsv` | 2,141 (`run`) | 2,093 (`run`) | the generator (item 92) |
| D2 integral `float64` cell written `7.0` | 1,239 direct, 8 derived, 328 of the shimmed | 1,215 direct, 10 derived, 302 of the shimmed plus 2 derived | Python (11.6) |
| D3 `compare_rows` ordering operator on `str` or `bool`, a row absent | 15 | 10 | not decided (gap) |
| D4 `median` of subnormal doubles | 5, plus 1 shimmed | 0 | Rust (11.6, 11.18.2) |
| D5 a cell error against an assertion validation error | 1 | 0 | not decided (gap) |

Verdicts never differed except through D3 and D5; no `steps` description, `columns`, or row-order difference appeared in 20,000 cases.

### D1. The generator writes `tables/<name>.tsv` for a tab-separated table

- Cases: run 1, seed 1795118323: 2,141 cases (every case with a `tsv` spec; first ones `c00002-logexp`, `c00005-rank-group`, `c00017-derive`, `c00023-join`); run 2, seed 2: 2,093 cases (`c00001-derive`, `c00002-logexp`, `c00005-rank-group`, `c00006-group`, ...). The full lists are the kept directories whose `python.stderr` ends in `FileNotFoundError: ... tables/<name>.csv`.
- Field: `run`. Rust: `ok`. Python: `exit 1: Traceback (most recent call last):`, ending `FileNotFoundError: [Errno 2] No such file or directory: '.../tables/tiny.csv'`.
- Sentence: item 92, "`conformance/checker/cases/<name>/` with `tables/*.csv`, `specs/*.json`, `steps.json`, `assertion.json`" and "A steps table name is the file stem of its file in `tables/`". Item 89 says the generator writes "each case ... in item 92's layout".
- Ruling sought: neither checker is wrong. The Python side opens `tables/<name>.csv`, which is the only file name item 92 gives; the Rust harness (`tests/common/case.rs`) reads whatever file stem is present, which also satisfies the sentence. The generator (`examples/gencases.rs`, `Table::file_name`) writes `<name>.tsv` when the spec's `format` is `tsv`, which item 92 does not provide for: the spec's `format` and `delimiter` decide how a file is read, the extension does not. Proposed fix, after the ruling: the generator writes every table file as `<name>.csv` whatever its delimiter, and a sentence in item 92 states "every table file is `tables/<name>.csv` whatever its `format`, the parse spec deciding the delimiter".
- Under the shim, those cases resolve as: run 1, 1,808 agree, 328 D2, 4 D2 derived (`c01017-exponent`, `c04890-float64`, `c06089-exponent`, `c09505-derive`), 1 D4 (`c02924-missing-group`); run 2, 1,789 agree, 302 D2, 2 D2 derived (`c02489-exponent`, `c07929-exponent`), none else.
- Resolved 2026-11-20, generator side only: after the owner's "make the changes yourself" on the CI failures, `Table::file_name` writes `<name>.csv` whatever the `format`. Checked with `scripts/differential.sh 400 1795118323`: no `run` row; the 68 differing cases are 67 D2 and 1 D3 (`c00087-join`), which is what the shim predicted for that range. The item 92 sentence above is still only proposed; `reference/checker/` was not touched.

### D2. An integral `float64` cell: Rust `7`, Python `7.0`

- Cases: run 1: 1,239 cases whose only differences are such cells (first ones `c00001-derive`, `c00008-select`, `c00026-float64`, `c00042-float64`), plus 4 whose derived column inherits the extra digit (`c01162-float64`, `c03577-exponent`, `c07945-exponent`, `c09786-float64`), plus under the shim 328 direct and 4 derived (`c01017-exponent`, `c04890-float64`, `c06089-exponent`, `c09505-derive`); run 2: 1,215 direct (first ones `c00010-float64`, `c00032-filter`, `c00035-sort-take`, `c00051-sort-take`) and 10 derived (`c00745-exponent`, `c01673-exponent`, `c02473-exponent`, `c03802-float64`, `c04249-exponent`, `c04713-exponent`, `c05625-exponent`, `c07209-exponent`, `c08442-float64`, `c09066-float64`), plus under the shim 302 direct and 2 derived (`c02489-exponent`, `c07929-exponent`).
- Fields: `final_rows` and `final_rows_hash`; `key_map_hash` too when such a column is a `group` `by` column or a key. Example, run 1 `c00042-float64`, column `score` declared `float64`, cell text `7`: Rust `{"t":"dec","v":"7"}`, Python `{"t":"dec","v":"7.0"}`. Derived example, run 1 `c07945-exponent`, `flag` is `float64` with cell `1000`, `derive` `` (`flag`*abs(12.5e-3)) ``: Rust `12.5000`, Python `12.50000` (the product's scale is the sum of the operands' scales, 11.6, so the extra digit propagates through every multiplication). Run 2 `c04249-exponent` `grp * (grp*`ts`)`: Rust `720000000000000000000.0`, Python `720000000000000000000.00`.
- Sentence: 11.6, "a parse spec may declare a column `float64`, in which case the value is the shortest round-trip decimal string of the IEEE value, in positional notation, with either zero written `0`", and 11.7, "In a `float64` column ... any other text is read as an IEEE double (exponent forms included) and stored shortest-round-trip".
- Ruling sought: Python is wrong under that sentence. `7` is a shorter decimal string than `7.0` and round-trips to the same double, and the clause about zero (`0`, not `0.0`) shows the form intended. `reference/checker/typed_values.py` `dec_from_double_shortest` builds the value from Python's `repr(x)`, which appends `.0` to an integral double; the digits are shortest, the formatting is not. Rust (`dec_from_f64_shortest`, `format!("{x:e}")`) writes `7`. The sentence could be read as naming Python's `repr` convention, so the proposed clarification is one example added to 11.6: "an integral double is written without a fraction (`7`, not `7.0`), so its scale is 0".
- No conformance case has an integral `float64` value (the cases use `1.5`, `0.25`, and the special tokens), which is why both implementations pass item 92's suite.
- Resolved 2026-11-20 after the owner lifted the no-edit rule for this entry: `dec_from_double_shortest` drops the trailing zeros of `repr`'s digit string before writing the value positionally, as `dec_from_f64_shortest` does on the Rust side. The 158 conformance directories still pass `--check`; `scripts/differential.sh 400 1795118323` then differs in one case, `c00087-join`, which is D3. The 11.6 example sentence above is still only proposed.

### D3. `compare_rows` with `lt`, `le`, `gt`, or `ge` on a `str` or `bool` column when a named row is absent: Rust `fails`, Python `E6004` at `/op`

- Cases: run 1: `c00087-join`, `c00503-join`, `c00611-sort-take`, `c01703-join`, `c02075-digit34`, `c03811-sort-take`, `c03812-rank-all`, `c06329-exponent`, `c06360-select`, `c06615-join`, `c06652-missing-group`, `c07261-missing-key-tie`, `c07994-float64`, `c08255-pipeline`, `c09448-select`; run 2: `c01216-filter`, `c01261-missing-key-tie`, `c02049-derive`, `c02197-rank-group`, `c02743-join`, `c03068-missing-group`, `c05305-exponent`, `c06121-exponent`, `c08687-pipeline`, `c09062-group`.
- Fields: `verdict` (Rust `fails`, Python `error`), `error` (Rust `null`, Python `{"code":"E6004","field":"/op"}`), and therefore every row-level field (Rust the table, Python `null`). Example, run 1 `c00503-join`: `inner` join then `filter` `x > x`; assertion `compare_rows`, left row `["ZZZ3"]` column `ann.v`, right row `["RB1"]` column `ann.v`, `op` `le`, both columns `str`; the left row names no final row.
- With both rows present the two implementations agree on `error` `E6004` at `/op` (checked on run 1 `c07994-float64` with its rows replaced by present keys), so the difference is one of order only: Python checks the operator against the column type before anything runs (`assertion.py` `validate_against_schema`, `_check_op_for_type`); Rust locates the rows first and reports `fails` for an absent one, and raises `E6004` only when it reaches the comparison.
- Sentences: 11.18.3 `compare_rows`, "Both cells located as for `value`; either absent is `fails`"; 11.18.3 "`op` on strings: `eq` and `ne` only, by NFC bytes; other operators and a present `tol`, zero or not, are `E6004`" (and the same row for booleans); 11.9, "An operator that the type of `expected` does not allow is `E6004` at `/op`", and the closed list of what is checked before anything runs when the schema is known: "`row` has one value per key column ...; `column` names a column ...; `expected` is of a type that compares with the column's ...; for `compare_rows` the two columns' types compare, else `E6004` at `/right/column`".
- Ruling sought: `checker.md` does not decide it. The static list in 11.9 does not include the operator against the column type for `compare_rows` (the `/op` check is stated for the type of `expected`, which `compare_rows` lacks), and the 11.18.3 `compare_rows` row puts locating before comparing, which supports Rust; but the Tolerance row makes the right cell play `expected`'s part for `compare_rows`, and every other type check 11.9 can make statically it makes statically, which supports Python. Proposed sentence for 11.9, after the `/right/column` clause: "and for `compare_rows` the operator and `tol` are checked against the left column's type as they are against `expected`'s type for `value`, `E6004` at `/op` or `/tol` before anything runs, so an operator the type does not allow is refused whether or not the rows exist." Under that sentence Rust is wrong and changes; under the opposite sentence ("the operator is checked when the two cells are compared, after both are located, so an absent row is `fails` first") Python is wrong.

### D4. `median` over members that are all the subnormal double `5e-324`: Rust `0`, Python `0.000...000494065645841247`

- Cases: run 1: `c02700-missing-group`, `c03626-float64`, `c06892-missing-group`, `c07564-missing-group`, `c09578-float64`, and under the shim `c02924-missing-group`; run 2: none.
- Fields: `final_rows` and `final_rows_hash` (`key_map_hash` too in `c06892-missing-group`, where the group rows also carry D2). Example, run 1 `c03626-float64`: `group` by `tissue` (`float64`, `nan` `value`, `inf` `missing`) with `a1 = median(tissue)`; the group whose two members are both the cell `5e-324` has `a1` Rust `{"t":"dec","v":"0"}`, Python `{"t":"dec","v":"0.` followed by 323 zeros and `494065645841247"}`. The group's own `by` value, the `float64` cell `5e-324` written positionally, is the same on both sides.
- Sentences: 11.6, "A value that reaches this encoding from an IEEE double (`log`, `exp`, `mean`, `median`) is first rounded to 15 significant digits from the double's exact binary value, not from its shortest decimal text, then written with the shortest digits that round-trip at that precision, in positional notation"; 11.18.2 `group`, "`mean` and `median` are DuckDB `DOUBLE` rounded to 15 significant digits and converted to `dec` per 11.6".
- Ruling sought: Rust is wrong under those sentences. The median of two equal doubles is that double, `5e-324` (exact binary value 4.9406564584124654...e-324), whose 15-significant-digit rounding is `4.94065645841247e-324`, which is what Python writes positionally. The Rust conversions on either side of DuckDB keep the value (`"0.000...005".parse::<f64>()` gives `5e-324` and `format!("{:.14e}", 5e-324)` gives `4.94065645841247e-324`, both run outside the checker), so the zero appears between the Arrow `Float64` copy and the `median` result inside DuckDB [likely]; the owner's ruling decides whether to find and fix that path or to state in 11.6 that a subnormal double result is written `0`. The result is a 324-digit positional `dec` either way the sentence is read, which is itself worth a ruling.

### D5. A data-file cell error against an assertion validation error: Rust `E3102`, Python `E6004` at `/op`

- Case: run 1 `c06122-float64`.
- Field: `error`. Rust `{"code":"E3102","field":null}` (the `float64` column `p-value` holds the cell `-1e309`, a finite number whose double overflows, 11.7); Python `{"code":"E6004","field":"/op"}` (the assertion is a `compare_rows` with `lt` on the `str` column `or`, the D3 check). Both verdicts are `error`.
- Sentences: 11.8, "When several checks would fail, the error reported is the first found in this order: the steps' shape, op by op; the steps against the cited specs, op by op; the assertion's shape (11.9); the assertion against the final table's schema (11.9)"; 11.7, "a finite number whose double overflows (`1e400`) being `E3102`"; item 92's harness description, which says nothing about when the tables are read.
- Ruling sought: `checker.md` does not decide it: the 11.8 order lists validation checks only and does not place a cell error of a cited table among them. In the product a table is parsed at `table.spec`, before any claim exists, so a cell error precedes every claim check; the Rust harness reproduces that by parsing every table before validating, and the Python harness validates first and reads a table only when a step needs it. Proposed sentence for item 92: "A harness parses every table in `tables/` before it validates the steps and the assertion, so a cell error (`E3102`, `field` `null`) is the case's error whatever the steps and assertion hold." Under that sentence Python is wrong. The case only surfaces together with D3; if D3 is ruled Rust's way the Python side reaches the table and the two agree without this sentence, which is still worth adding.
