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
- **Ruling (2026-11-20).** Clarification in: item 92 now says every table file is `tables/<name>.csv` whatever its `format`, the parse spec deciding the delimiter.

### D2. An integral `float64` cell: Rust `7`, Python `7.0`

- Cases: run 1: 1,239 cases whose only differences are such cells (first ones `c00001-derive`, `c00008-select`, `c00026-float64`, `c00042-float64`), plus 4 whose derived column inherits the extra digit (`c01162-float64`, `c03577-exponent`, `c07945-exponent`, `c09786-float64`), plus under the shim 328 direct and 4 derived (`c01017-exponent`, `c04890-float64`, `c06089-exponent`, `c09505-derive`); run 2: 1,215 direct (first ones `c00010-float64`, `c00032-filter`, `c00035-sort-take`, `c00051-sort-take`) and 10 derived (`c00745-exponent`, `c01673-exponent`, `c02473-exponent`, `c03802-float64`, `c04249-exponent`, `c04713-exponent`, `c05625-exponent`, `c07209-exponent`, `c08442-float64`, `c09066-float64`), plus under the shim 302 direct and 2 derived (`c02489-exponent`, `c07929-exponent`).
- Fields: `final_rows` and `final_rows_hash`; `key_map_hash` too when such a column is a `group` `by` column or a key. Example, run 1 `c00042-float64`, column `score` declared `float64`, cell text `7`: Rust `{"t":"dec","v":"7"}`, Python `{"t":"dec","v":"7.0"}`. Derived example, run 1 `c07945-exponent`, `flag` is `float64` with cell `1000`, `derive` `` (`flag`*abs(12.5e-3)) ``: Rust `12.5000`, Python `12.50000` (the product's scale is the sum of the operands' scales, 11.6, so the extra digit propagates through every multiplication). Run 2 `c04249-exponent` `grp * (grp*`ts`)`: Rust `720000000000000000000.0`, Python `720000000000000000000.00`.
- Sentence: 11.6, "a parse spec may declare a column `float64`, in which case the value is the shortest round-trip decimal string of the IEEE value, in positional notation, with either zero written `0`", and 11.7, "In a `float64` column ... any other text is read as an IEEE double (exponent forms included) and stored shortest-round-trip".
- Ruling sought: Python is wrong under that sentence. `7` is a shorter decimal string than `7.0` and round-trips to the same double, and the clause about zero (`0`, not `0.0`) shows the form intended. `reference/checker/typed_values.py` `dec_from_double_shortest` builds the value from Python's `repr(x)`, which appends `.0` to an integral double; the digits are shortest, the formatting is not. Rust (`dec_from_f64_shortest`, `format!("{x:e}")`) writes `7`. The sentence could be read as naming Python's `repr` convention, so the proposed clarification is one example added to 11.6: "an integral double is written without a fraction (`7`, not `7.0`), so its scale is 0".
- No conformance case has an integral `float64` value (the cases use `1.5`, `0.25`, and the special tokens), which is why both implementations pass item 92's suite.
- Resolved 2026-11-20 after the owner lifted the no-edit rule for this entry: `dec_from_double_shortest` drops the trailing zeros of `repr`'s digit string before writing the value positionally, as `dec_from_f64_shortest` does on the Rust side. The 158 conformance directories still pass `--check`; `scripts/differential.sh 400 1795118323` then differs in one case, `c00087-join`, which is D3. The 11.6 example sentence above is still only proposed.
- **Ruling (2026-11-20).** Clarification in: 11.6 now says an integral double is written without a fraction (the double 7.0 is written 7, scale 0, not 7.0).

### D3. `compare_rows` with `lt`, `le`, `gt`, or `ge` on a `str` or `bool` column when a named row is absent: Rust `fails`, Python `E6004` at `/op`

- Cases: run 1: `c00087-join`, `c00503-join`, `c00611-sort-take`, `c01703-join`, `c02075-digit34`, `c03811-sort-take`, `c03812-rank-all`, `c06329-exponent`, `c06360-select`, `c06615-join`, `c06652-missing-group`, `c07261-missing-key-tie`, `c07994-float64`, `c08255-pipeline`, `c09448-select`; run 2: `c01216-filter`, `c01261-missing-key-tie`, `c02049-derive`, `c02197-rank-group`, `c02743-join`, `c03068-missing-group`, `c05305-exponent`, `c06121-exponent`, `c08687-pipeline`, `c09062-group`.
- Fields: `verdict` (Rust `fails`, Python `error`), `error` (Rust `null`, Python `{"code":"E6004","field":"/op"}`), and therefore every row-level field (Rust the table, Python `null`). Example, run 1 `c00503-join`: `inner` join then `filter` `x > x`; assertion `compare_rows`, left row `["ZZZ3"]` column `ann.v`, right row `["RB1"]` column `ann.v`, `op` `le`, both columns `str`; the left row names no final row.
- With both rows present the two implementations agree on `error` `E6004` at `/op` (checked on run 1 `c07994-float64` with its rows replaced by present keys), so the difference is one of order only: Python checks the operator against the column type before anything runs (`assertion.py` `validate_against_schema`, `_check_op_for_type`); Rust locates the rows first and reports `fails` for an absent one, and raises `E6004` only when it reaches the comparison.
- Sentences: 11.18.3 `compare_rows`, "Both cells located as for `value`; either absent is `fails`"; 11.18.3 "`op` on strings: `eq` and `ne` only, by NFC bytes; other operators and a present `tol`, zero or not, are `E6004`" (and the same row for booleans); 11.9, "An operator that the type of `expected` does not allow is `E6004` at `/op`", and the closed list of what is checked before anything runs when the schema is known: "`row` has one value per key column ...; `column` names a column ...; `expected` is of a type that compares with the column's ...; for `compare_rows` the two columns' types compare, else `E6004` at `/right/column`".
- Ruling sought: `checker.md` does not decide it. The static list in 11.9 does not include the operator against the column type for `compare_rows` (the `/op` check is stated for the type of `expected`, which `compare_rows` lacks), and the 11.18.3 `compare_rows` row puts locating before comparing, which supports Rust; but the Tolerance row makes the right cell play `expected`'s part for `compare_rows`, and every other type check 11.9 can make statically it makes statically, which supports Python. Proposed sentence for 11.9, after the `/right/column` clause: "and for `compare_rows` the operator and `tol` are checked against the left column's type as they are against `expected`'s type for `value`, `E6004` at `/op` or `/tol` before anything runs, so an operator the type does not allow is refused whether or not the rows exist." Under that sentence Rust is wrong and changes; under the opposite sentence ("the operator is checked when the two cells are compared, after both are located, so an absent row is `fails` first") Python is wrong.
- **Ruling (2026-11-20).** Static. Rust changes. An assertion's type correctness cannot depend on which rows this version of the data contains; the M5 ruling made the check static for value, and compare_rows follows. 11.9 gains the sentence in section 2 below.
- Applied 2026-11-20: 11.9 and the 11.18.3 `compare_rows` row carry the sentence; `Assertion::check` in `tm-formats` refuses the operator at `/op`, then `tol` at `/tol`, against the left column's type, in the reference checker's order (README reading 15). Unit test `compare_rows_operator_and_tol_are_checked_against_the_left_column`. The order for `value` is D6.

### D4. `median` over members that are all the subnormal double `5e-324`: Rust `0`, Python `0.000...000494065645841247`

- Cases: run 1: `c02700-missing-group`, `c03626-float64`, `c06892-missing-group`, `c07564-missing-group`, `c09578-float64`, and under the shim `c02924-missing-group`; run 2: none.
- Fields: `final_rows` and `final_rows_hash` (`key_map_hash` too in `c06892-missing-group`, where the group rows also carry D2). Example, run 1 `c03626-float64`: `group` by `tissue` (`float64`, `nan` `value`, `inf` `missing`) with `a1 = median(tissue)`; the group whose two members are both the cell `5e-324` has `a1` Rust `{"t":"dec","v":"0"}`, Python `{"t":"dec","v":"0.` followed by 323 zeros and `494065645841247"}`. The group's own `by` value, the `float64` cell `5e-324` written positionally, is the same on both sides.
- Sentences: 11.6, "A value that reaches this encoding from an IEEE double (`log`, `exp`, `mean`, `median`) is first rounded to 15 significant digits from the double's exact binary value, not from its shortest decimal text, then written with the shortest digits that round-trip at that precision, in positional notation"; 11.18.2 `group`, "`mean` and `median` are DuckDB `DOUBLE` rounded to 15 significant digits and converted to `dec` per 11.6".
- Ruling sought: Rust is wrong under those sentences. The median of two equal doubles is that double, `5e-324` (exact binary value 4.9406564584124654...e-324), whose 15-significant-digit rounding is `4.94065645841247e-324`, which is what Python writes positionally. The Rust conversions on either side of DuckDB keep the value (`"0.000...005".parse::<f64>()` gives `5e-324` and `format!("{:.14e}", 5e-324)` gives `4.94065645841247e-324`, both run outside the checker), so the zero appears between the Arrow `Float64` copy and the `median` result inside DuckDB [likely]; the owner's ruling decides whether to find and fix that path or to state in 11.6 that a subnormal double result is written `0`. The result is a 324-digit positional `dec` either way the sentence is read, which is itself worth a ruling.
- **Ruling (2026-11-20).** Rust is wrong. Fix the Rust path; if the flush is inside DuckDB's median and cannot be routed around, median is computed in Rust from the member rows as sum is, with the group row edited first. 11.6 gains: a subnormal double is a value and is never flushed to zero. A 324-digit positional dec is acceptable output: 11.6's no-exponent rule stands, since changing it would change the hash of every value; the static view may display such a value in scientific notation (ruling 32).
- Applied 2026-11-20: the flush is DuckDB's `median` interpolation, not the input path. Through the Checker, `mean` over the same members is right for every count, `median` is right for an odd count and wrong for an even one, and at `1e-310` it loses low bits without reaching zero, so it cannot be routed around short of giving DuckDB other data. `median` is now computed in Rust from the `members` list (`eval::median`, the Python formula: sorted doubles, the middle one or `lo + (hi - lo) * 0.5`), with 11.6, the 11.18.2 Where and `group` rows edited first; `mean` stays DuckDB `avg`. Tests: `median_keeps_a_subnormal_and_interpolates_the_even_case` (unit) and `mean_and_median_keep_a_subnormal_double` (counts 2, 3, and 4 through the Checker). The four SQL snapshots with `median` change, since the statement no longer carries it.

### D5. A data-file cell error against an assertion validation error: Rust `E3102`, Python `E6004` at `/op`

- Case: run 1 `c06122-float64`.
- Field: `error`. Rust `{"code":"E3102","field":null}` (the `float64` column `p-value` holds the cell `-1e309`, a finite number whose double overflows, 11.7); Python `{"code":"E6004","field":"/op"}` (the assertion is a `compare_rows` with `lt` on the `str` column `or`, the D3 check). Both verdicts are `error`.
- Sentences: 11.8, "When several checks would fail, the error reported is the first found in this order: the steps' shape, op by op; the steps against the cited specs, op by op; the assertion's shape (11.9); the assertion against the final table's schema (11.9)"; 11.7, "a finite number whose double overflows (`1e400`) being `E3102`"; item 92's harness description, which says nothing about when the tables are read.
- Ruling sought: `checker.md` does not decide it: the 11.8 order lists validation checks only and does not place a cell error of a cited table among them. In the product a table is parsed at `table.spec`, before any claim exists, so a cell error precedes every claim check; the Rust harness reproduces that by parsing every table before validating, and the Python harness validates first and reads a table only when a step needs it. Proposed sentence for item 92: "A harness parses every table in `tables/` before it validates the steps and the assertion, so a cell error (`E3102`, `field` `null`) is the case's error whatever the steps and assertion hold." Under that sentence Python is wrong. The case only surfaces together with D3; if D3 is ruled Rust's way the Python side reaches the table and the two agree without this sentence, which is still worth adding.
- **Ruling (2026-11-20).** Parse first. Python changes, confined to the order in refcheck.py. The product parses a table at table.spec, before any claim exists, so a claim is never validated against a table that did not parse; the harness mirrors that.
- Applied 2026-11-20: item 92 carries the sentence; `refcheck.py` parses every `tables/*.csv` with its spec before validating anything, and README reading 11 says so. `c06122-float64` now gives `E3102` on both sides.

## Latent differences found while turning the README readings into design rules (2026-11-20)

None of these showed in the 30,000 cases run so far: each needs an input the generator does not draw. They were found by reading both implementations side by side (the Rust lines were checked by hand unless an entry says otherwise). Each is a specification gap or a one-sided defect, as stated; nothing is fixed.

### D6. `value` with a `str` or `bool` `expected`, an ordering operator, and a `tol`: Rust `E6004` at `/tol`, Python `E6004` at `/op`

- Cases: none run. Both checks are static; the two sides report different first errors.
- Sentence: 11.9, "An operator that the type of `expected` does not allow is `E6004` at `/op`, and a `tol` it does not allow is `E6004` at `/tol`", which gives no order. Rust `check_static` (`tm-formats` `assertion.rs`) tests `tol` first; Python `_expected` tests `op` first and `tol` after. The same holds for `between` with `str` bounds plus a `tol`.
- Proposed sentence for 11.9: "when both fail, `/op` is reported before `/tol`". Under it Rust changes (two `if` blocks swap). The D3 code already uses that order for `compare_rows`.
- **Ruling (2026-11-20).** `/op` before `/tol`. Rust changes (the two checks swap). 11.9 says so; the D3 code already uses that order for `compare_rows`.
- Applied 2026-11-20: `check_static` in `tm-formats` tests the operator first. Unit test `rulings_of_2026_11_20_on_the_shape_check`.

### D7. The version handle form in `tables`: Rust `F<n>@v<m>` only, Python `<text without @ or whitespace>@v<n>`

- Cases: none run; the generator writes `F1@v1`-style handles.
- Sentence: 11.8 says "`E6004` for a member of the wrong JSON type or form (a string that is not a version handle)" and leaves the form to the handle grammar elsewhere in the design, which Rust's `VersionHandle::parse` (`id.rs`) follows: `F`, a positive integer, `@v`, a positive integer. Python (README reading 1) accepts `X7@v2` and `a.csv@v1`.
- Proposed: 11.8 names the form, "a version handle is `F<n>@v<m>` with both positive integers (11.x)". Python changes. Item 92 already says the harness ignores the handle, so no case changes.
- **Ruling (2026-11-20).** `F<n>@v<m>`, 11.16's form. Python changes. 11.8 names the form.
- Applied 2026-11-20: `ops.py` `HANDLE_RE` is `F[1-9][0-9]*@v[1-9][0-9]*`; README reading 1 says so.

### D8. `derive` whose `as` is already a column and whose `expr` also fails: Rust types the expression first, Python checks `as` first

- Cases: none run.
- Sentence: 11.8's order is "the steps against the cited specs, op by op" with no order inside an op. Rust (`steps.rs`, `Op::Derive`) raises the expression's `E6005`/`E6004` at `/ops/<i>/expr` before `E6002` at `/ops/<i>/as`; Python (reading 3) the reverse.
- Proposed: inside one op, members are checked in the order 11.8's table lists them (`as` before `expr`), so `/ops/<i>/as` comes first. Under it Rust changes (one line moves). The opposite sentence is as cheap.
- **Ruling (2026-11-20).** `as` before `expr`: members are checked in the order 11.8's op table lists them. Rust changes. 11.8 states the within-op order.
- Applied 2026-11-20: `Steps::check` in `tm-formats` refuses a present `as` at `/ops/<i>/as` before typing `expr`. Unit test `rulings_of_2026_11_20_against_the_specs`.

### D9. Two `E6002` pointers for an added column already present: `group` `as` equal to `member_count` or `member_keys_hash`, and a `join` prefixed column

- Cases: none run.
- Sentence: 11.8, "a step that would add a column whose name is already present" is `E6002` with `field` "the JSON Pointer of the offending member". Rust reports both at `/ops/<i>` (`steps.rs`: the two fixed columns are added after the aggregates; the join prefix clash is raised with the op's pointer); Python at `/ops/<i>/aggregates/<j>/as` and `/ops/<i>/right` (reading 4). The `rank` and the aggregate-repeats-`by` cases agree and are now in 11.8.
- Proposed: the finer pointers, as 11.12 asks for the offending member: `/ops/<i>/aggregates/<j>/as` and `/ops/<i>/right`. Rust changes.
- **Ruling (2026-11-20).** The finer pointers, `/ops/<i>/aggregates/<j>/as` and `/ops/<i>/right`, as 11.12 asks for the offending member. Rust changes. 11.8 lists both.
- Applied 2026-11-20: `Steps::check` reports both at the finer pointer. Same unit test as D8.

### D10. Unary minus and `abs` on a `dec` with more than 34 significant digits: Rust rounds to 34, Python is exact

- Cases: none run; it needs a 35-to-38-digit cell (the `digit34` bucket stays at 34) negated or passed to `abs`.
- Sentence: 11.18.2 Precision, "`dec` results round to 34 significant digits, half-even", and General Decimal Arithmetic, whose `minus` and `abs` apply the context. Rust `eval.rs` runs both through `fix` (rounds, may raise `E5002`); Python (reading 6) returns the operand's digits unchanged.
- Decided: Python is wrong under the Precision row. One line in `exprs.py` after the owner confirms, since `reference/checker/` stays closed.
- **Ruling (2026-11-20).** Python is wrong: General Decimal Arithmetic's `minus` and `abs` apply the context. Python changes. The 11.18.2 Precision row says so.
- Applied 2026-11-20: `exprs.py` runs both through the shared context (`CONTEXT.minus`, `CONTEXT.abs`), Overflow being `E5002` as for the other arithmetic; README reading 6 says so.

### D11. A `float64` cell with surrounding whitespace: Rust reads it, Python refuses it

- Cases: none run; the generator writes no padding.
- Sentence: 11.7 gives the tokens and "any other text is read as an IEEE double (exponent forms included)"; it does not say whether whitespace is text. Rust `tm-files` `parse.rs` trims before `parse::<f64>()`; Python (reading 10) matches `[+-]?(\d+(\.\d*)?|\.\d+)([eE][+-]?\d+)?` exactly and gives `E3102`.
- Proposed: a cell is read as written; 11.7 gains the regular expression above, and a padded cell is `E3102`. Rust changes (drop the `trim`). Whether other column types trim in either implementation is not checked.
- **Ruling (2026-11-20).** A cell is read as written in every column type; a padded cell is `E3102`. Rust changes (drop the `trim`) and confirms no other column type trims. Item 47 already names RFC 4180, where a space is part of the field. 11.7 says so.
- Applied 2026-11-20: `tm-files` `parse.rs` no longer trims a `float64` cell (nor the draft-scale read of a `dec` cell). Checked by hand that `int`, `dec`, `bool`, `date` padded cells were already `E3102` on both sides and that a padded `str` cell keeps its spaces on both.

### D12. `between` whose `lo` and `hi` do not compare: Rust `E6004` at `/expected/1`, Python at `/expected`

- Cases: none run.
- Sentence: 11.9 says `expected` is `[lo, hi]` and gives no pointer for the pair. Rust `assertion.rs` points at `/expected/1`; Python (reading 16) at `/expected`.
- Proposed: `/expected/1`, the member that does not compare with the one before it. Python changes.
- **Ruling (2026-11-20).** `/expected/1`, the member that does not compare with the one before it. Python changes. 11.9 says so.
- Applied 2026-11-20: `assertion.py` `_expected` points at `/expected/1`; README reading 16 says so.

### D13. A `list`, `nan`, or `inf` in `row`: Python refuses it at the shape check, Rust at the check against the schema

- Cases: none run. Same code and pointer (`E6004` at `/row/<i>`, now in 11.9), different phase: Python (reading 17) refuses it while reading the assertion; Rust `row()` accepts any typed value and `Assertion::check` refuses it, after a row-arity error at `/row` if there is one, and never for an eval case, which has no schema.
- Proposed: the shape check refuses it (11.9's shape list), so an eval case with `nan` in `row` is `E6004` on both sides. Rust changes.
- **Ruling (2026-11-20).** The shape check refuses it, so an eval case refuses it too. Rust changes. 11.9 says so.
- Applied 2026-11-20: `row()` in `tm-formats` `assertion.rs` refuses `list`, `nan`, and `inf` with `E6004` at `/row/<i>` while reading the assertion. Unit test `rulings_of_2026_11_20_on_the_shape_check`.

### D14. A `tables` entry with no spec that no op uses: Python `E6002` at `/tables/<name>`, Rust silent

- Cases: none run; the generator writes a spec per table.
- Sentence: 11.8 says `E6005`, operand types and the join cover are checked "against the cited tables' parse specs" and does not say whether an unused entry needs one. Rust `Steps::check` looks a spec up only for `from` and each `join` `right`; Python (reading 18) checks every entry first.
- Proposed: every `tables` entry names a table with a spec, `E6002` at `/tables/<name>` otherwise, checked before the ops. Rust changes.
- **Ruling (2026-11-20).** Every `tables` entry names a table with a spec, `E6002` at `/tables/<name>`, checked before the ops: a declared input nothing uses is a mistake the author should hear about. Rust changes. 11.8 lists it.
- Applied 2026-11-20: `Steps::check` looks every `tables` entry up before the ops. Unit test `rulings_of_2026_11_20_against_the_specs`.

### D15. A header lacking a spec column: Rust `E3103` at `/columns/<i>/name`, Python at `/columns/<i>`

- Cases: none run.
- Sentence: 11.7, "a column the file's header does not have, is `E3103`; `field` points at the offending member". Rust `ParseSpec::check_header` points at the name; Python (reading 21) at the column object.
- Proposed: `/columns/<i>/name`, the member whose value is not in the header. Python changes.
- **Ruling (2026-11-20).** `/columns/<i>/name`, the member whose value is not in the header. Python changes. 11.7 says so.
- Applied 2026-11-20: `tables.py` `read_table` points at `/columns/<i>/name`; README reading 21 says so.

### D16. A key cell that is `missing`, `nan`, or `inf`: Rust `E3102`, Python accepts the row

- Cases: none run; checked by hand with a two-row table whose key column holds `NA`: Rust `error` `E3102` (`field` null), Python `holds` with a `missing` key component in `final_rows`.
- Sentence: 11.7 says only that "Key uniqueness is checked at `table.spec`"; 11.18.2 Initial order says "a key holds `missing` only after a `group` on a column with missing values", which Rust `tm-files` `parse.rs` enforces (`E3102`, message "a key value is missing") and Python does not (reading 20 even provides for matching `nan` and `inf` key components).
- Proposed sentence for 11.7: "a key cell that is `missing`, or `nan` or `inf` under a `value` setting, is `E3102`, so a key holds `missing` only after a `group`". Python changes; reading 20 becomes moot.
- **Ruling (2026-11-20).** Python is wrong under text already in the design (item 47 and 11.18.2 Initial order): a key cell that is `missing`, `nan`, or `inf` is `E3102`. Python changes; README reading 20 is withdrawn. No design edit.
- Applied 2026-11-20: `tables.py` `read_table` refuses a key cell that is `missing`, `nan`, or `inf` with `E3102`, `field` null; README reading 20 is marked withdrawn.

### D17. Shape-phase order inside one op, the edge cases (not checked by hand)

- From the side-by-side reading, not verified at the lines: a bad `scope` together with an undefined member (Python checks `scope` first, Rust lists undefined members first); a negative `v` (Rust `E6004`, Python `E6002`); duplicate names in a column list (Rust `E6002`, Python not checked); an empty `as` (Rust `E6004`); and the order undefined members are listed in (Rust sorted, Python as written). 11.8 gives the member order and no more.
- Proposed: a ruling on each after a check at the lines, or a generator bucket that draws malformed ops so the differential finds them. Not ruled.
- **Ruling (2026-11-20).** Within one op: `op`, `v`, required members in the op table's order, `rank`'s `scope` and `group_by` rule, undefined members in the order written, then member values. A `v` that is not a positive integer is `E6004` (form); a column list with a repeated name is `E6002`, the code 11.8 already uses for a step that adds a present name; an empty `as` is `E6004`. Rust changes the undefined-member order (written, not sorted) and the `scope`-before-undefined order; Python changes the negative `v` code and adds the repeated-name and empty-`as` checks. 11.8 states all of it. A generator bucket that draws malformed ops is added to item 89 so the differential reaches these.
- Applied 2026-11-20: Rust `read_op` requires a positive `v` (`E6004`), checks `rank`'s `scope` after the required members and before the undefined ones, and reports undefined members in the order written (`schema::undefined`, canonical order kept for every other document); it also refuses a repeated column in `sort.by` at `/ops/<i>/by/<j>/column`, which Python now does too (neither side checked `sort.by` before; the other column lists were already `E6002` on the Rust side). Python's `v`, repeated-name, and empty-`as` checks are in `ops.py`. Unit test `rulings_of_2026_11_20_on_the_shape_phase`. The generator's `malformed` bucket (item 89) draws one of seventeen defects per case so the differential reaches every ruling of this section.

### D18. A blank line in a table file: Python skips it, Rust not established

- Cases: none run; the generator writes none.
- Sentence: item 47 names RFC 4180, which has no blank lines; README reading 21 skips them; what `tm-files` does was not checked when the readings were written into the design.
- **Ruling (2026-11-20).** A blank line is skipped: it can be a valid row in no table, and exports commonly end with one. Rust confirms or changes `tm-files`; 11.7 says so.
- Applied 2026-11-20: confirmed by hand, no change: `tm-files` reads through DuckDB `read_csv`, which skips a blank line in the middle and at the end, as `tables.py` does; the `malformed` bucket writes one.

## The CI differential (2026-11-20)

The CI job runs `scripts/differential.sh 10000 <run id>` on `ubuntu-24.04` (x64) and `ubuntu-24.04-arm` (arm64) with the workflow run's id as the seed, after rulings D1 to D18 were applied on both sides.

| Run | Seed | Cases | Differing cases | Differing fields | Distinct causes | Runner classes |
| --- | --- | --- | --- | --- | --- | --- |
| CI 44803194352 | 44803194352 | 10,000 | 1 | 2 | 1 (D19) | both, the same case with the same hashes |

Host runs of the `malformed` bucket alone, before the CI run: `gencases` at seeds 1795118323, 2, and 3, the bucket's 588 cases of each 10,000, both checkers on every case. Of the 588, 459, 444, and 436 cases are refused by `tm_checker::rules::validate`, which is what the count is: 13 of the bucket's 17 defects are steps or assertion validation failures (the other four, a padded cell, a blank line, a `missing` key cell, and a 35-digit `dec` under unary minus or `abs`, are data or evaluation cases). The differential showed no `error` row and no `run` row on any of the three runs.

### D19. `coalesce` over an `int` and a `dec` column, multiplied: Rust rounds at 34 digits, Python multiplies the `int` cells exactly

- Case: `c02857-derive`, the only differing case of the CI run, found on both runner classes with the same hashes on each side. Reproduced here with `gencases --seed 44803194352 --start 2857 --n 1`.
- Fields: `final_rows` and `final_rows_hash`. Table `meta` (`gene_id` `str` key; `or` `int`; `n` `dec`, scale 0): `EGFR` has `or` `123456789012345678901234567` and `n` missing; `MDM2` has `or` missing and `n` `-10`. The first op is `derive d1 = (coalesce(`or`,`n`)) * coalesce(`or`, `or`)`. `EGFR`'s `d1`: Rust `{"t":"dec","v":"15241578753238836750495351342783110000000000000000000"}`, the exact square rounded to 34 significant digits; Python `{"t":"dec","v":"15241578753238836750495351342783114345526596755677489"}`, the exact 53-digit square. `final_rows_hash` Rust `sha256:592886985a52a33af9dd70b9695d88c828fa4d4c9d013184be61dc940a0932c3`, Python `sha256:95647c7f46318a2d8f8f2df0fcad6b1588303dc5246b24b5f39d631d186b6385`. `MDM2`'s `d1` is missing on both sides; the verdict (`fails`: 2 rows, `count` `lt` 2), `steps`, `columns`, `key_map_hash`, and `error` agree.
- Sentences: 11.18.2 Types, "`int` combined with `dec` gives `dec`" and "`coalesce` gives its arguments' common type, `int` and `dec` widening to `dec`"; 11.18.2 Precision, "`dec` results round to 34 significant digits, half-even" and "`int` results are of arbitrary size and never overflow". Both sides give the column the static type `dec`; Python evaluated each row by the run-time tags of the values `coalesce` chose, two `int` cells here, and multiplied them as `int`.
- **Ruling (2026-11-20).** Rust is right. Arithmetic runs in the expression's static type (11.18.2 Types). `coalesce(or, n)` is `dec` because `n` is a `dec` column, so `d1` is `dec` and rounds at 34 significant digits, whichever branch `coalesce` took at run time. Python multiplied two integral run-time values as ints, exact.
- Applied 2026-11-20: the 11.18.2 Precision row says arithmetic is performed in the expression's static type. Python (`exprs.bind`, under a ruling-specified exception to its no-edit rule, logged in `docs/decisions.md`) marks every `coalesce` whose static type is `dec` so the value it chooses is a `Decimal` in the context from then on; `ops.py` binds a `filter` or `derive` expression once per step. On this case the two sides now agree on all seven fields. Conformance case `derive_coalesce_static_dec` (an 18-digit `int` cell squared through `coalesce`, its `expected.json` equal to the Rust output); the generator's `digit34` bucket draws `coalesce` over an `int` and a `dec` column with a product past 34 digits in one arm of six.

## Conformance cases from the independent case writer (2026-11-23)

### D20. An `int` cell of 401 digits: Rust `E3102`, Python reads it and takes its `log`

- Case: `log_of_int_beyond_double` (b48e8be), written by the independent case writer to exercise the 11.18.2 Domain sentence "An `int` too large for a double is infinity to the `log` family, so its logarithm is `missing`".
- Field: `verdict` and `error`, and with them every row-level field. Its table holds an `int` cell `10^400` (401 digits). Rust: verdict `error`, `{"code":"E3102","field":null}`, from parsing the cell. Python: the case's `expected.json`, the cell read at full width and its `log` `missing`.
- Sentences: item 47, "an integer wider than 38 digits is `E3102`" (`int` maps to `Decimal128(38,0)`); 11.6, `int` values are of arbitrary size; 11.18.2 Domain, as above. Item 47 is not among the sections `docs/spec/checker.md` copies, so the case writer and the Python checker never saw the limit, and the Domain sentence read as if a cell could hold such an `int`.
- **Ruling (2026-11-23).** Rust is right. Item 47 is not among the sections `docs/spec/checker.md` copies, so the case writer and the Python checker never saw the limit. 11.7 gains: an `int` cell wider than 38 digits is `E3102` (item 47 maps `int` to `Decimal128(38,0)`); 11.6's arbitrary size applies to computed `int` values, which never leave Rust or Python as Arrow, copied into 11.7 so `checker.md` carries it, as ruling 12 did for the NaN tokens. The Domain sentence gains "(only a computed `int` can be: a cell is at most 38 digits)". Python's `tables.py` refuses an `int` cell wider than 38 digits with `E3102`, field null, the message naming the row and column. The independent case writer rewrites the case so the huge `int` comes from a `derive`.
- Applied 2026-11-23: the design and both spec copies carry the two sentences (89f54a5). `tables.py` refuses an `int` cell whose absolute value has more than 38 digits; 38 nines in either sign is read, `±10^38` is `E3102` with `field` null and `row <r>, column '<name>'` in the message. On `log_of_int_beyond_double` the Python checker now gives `E3102` like Rust, so the case fails `--check` on both sides until the case writer rewrites it; every other conformance directory passes. The item 89 `logexp` arm from f2055ae draws its 310-digit `int` as a literal inside the `derive` expression, not as a cell, so it is a computed `int` and agreed on both sides in the 3,400-case run; it is unchanged.

## Review of p7-context (2026-11-24)

From `reviews/2026-11-24-p7-context.md`, findings A and B: two differences that predate the branch the review covered, found while probing ruling 56's literal-range check.

### D21. A literal outside the context's range beside a second fault: Python `E6004` at the expression, Rust the second fault

- Cases: the review's four, run as steps schema cases (`reference/checker/validate.py`, and `Steps::from_json` then `Steps::check` in `tm-formats`, which needs no DuckDB) over one table `t` (`k` `str` key, `x` `dec` scale 2):

  | Case | Python | Rust |
  | --- | --- | --- |
  | a. `filter` `x < 1e6145 +` | `E6004` `/ops/0/where` | `E6006` `/ops/0/where` |
  | b. `filter` `x < 1e6145`, then a `sort` with no `by` | `E6004` `/ops/0/where` | `E6002` `/ops/1/by` |
  | c. `filter` `nope < 1e6145` | `E6004` `/ops/0/where` | `E6005` `/ops/0/where` |
  | d. `derive` `as` `x` (already present), `expr` `x * 1e6145` | `E6004` `/ops/0/expr` | `E6002` `/ops/0/as` |

- Field: `error`. Python checks a numeric literal's range as `exprs.parse` reads it, in the steps' shape pass (`ops._expression`); Rust parses in the shape pass without the check (`steps.rs`, `expr::parse`) and makes it in `Expr::type_of`, in the pass against the cited specs, where a `derive` checks its `as` before its `expr`. The difference dates from ruling 36.
- Sentences: 11.8, "Checks that need no parse spec are made wherever steps are read; `E6005`, the operand types, and the join cover are checked against the cited tables' parse specs before anything runs", and its order "the steps' shape, op by op; the steps against the cited specs, op by op". 11.18.1's range check needs no parse spec. D13 settled a shape check against a schema check the same way.
- **Ruling (2026-11-24).** The literal range check is a shape check (11.8: "checks that need no parse spec are made wherever steps are read"; precedent D13). Rust changes; Python already does this.
- Applied 2026-11-24: `tm-formats`' parser checks a numeric literal's range as it reads the `INT` or `DEC` token (`expr::parse`), and `Expr::type_of` no longer does, so the check runs in the steps' shape pass, as in Python; 11.18.1 states the order within one expression. The four cases above, rerun on the Rust side: a. `E6004` `/ops/0/where`; b. `E6004` `/ops/0/where`; c. `E6004` `/ops/0/where`; d. `E6004` `/ops/0/expr`, equal to Python's. Unit test `steps::tests::literal_range_is_a_shape_check` (the four, a syntax error before the literal and a token error after it, both `E6006`); conformance case `steps/invalid/derive_existing_as_literal_out_of_range` (case d).

### D22. A literal whose exponent has more than 18 digits: Python crashes, Rust `E6004`

- Case: `filter` `x < 1e99999999999999999999` (a 20-digit exponent) on the table of D21. Python: `refcheck.py` and `validate.py` exit 1 with `decimal.InvalidOperation` from `Decimal(text)` in `exprs.number_literal`, the exponent being past libmpdec's `MAX_EMAX` (999999999999999999, 18 digits). Rust: `E6004` `/ops/0/where` (`adjusted_exponent` gives `None` when the exponent does not fit an `i64`).
- Field: `run` (Python gives no output).
- Sentence: 11.18.1, "A numeric literal whose adjusted exponent ... is above `Emax` 6144 or below `Emin` -6143 is `E6004` at validation". The literal is out of range; only the reference implementation's way of computing the adjusted exponent fails.
- **Ruling (2026-11-24).** Python checks an exponent's digit count on the text before calling `Decimal`, so `x < 1e99999999999999999999` is `E6004`, not a crash. A ruling-specified exception to the `reference/checker/` no-edit rule.
- Applied 2026-11-24, corrected 2026-11-26: `exprs.number_literal` refuses a literal whose exponent has more than 18 significant digits (leading zeros and the sign not counted) with `E6004` before calling `Decimal`; the limit `Decimal` enforces is on the adjusted exponent, not the exponent's digit count, so an exponent of 18 digits or fewer does not by itself reach `Decimal` safely, and since 2026-11-26 the adjusted exponent is computed from the text with Python ints and checked against the unchanged bounds, `Decimal` being called only for a literal that passed (the re-audit below). Rerun on 2026-11-24: `x < 1e99999999999999999999` and `x < 1e-99999999999999999999` are `E6004`, `x < 1e999999999999999999` (18 digits) is `E6004` through `Decimal`, and `x < 1e000000000000000000000000001` (exponent 1) is valid, as in Rust, whose `i64` parse also skips leading zeros. Logged in `docs/decisions.md` as a ruling-specified exception.
- The 18-digit threshold is libmpdec's `MAX_EMAX` on 64-bit builds (999999999999999999); 32-bit builds, whose `MAX_EMAX` is 425000000, are out of scope.
- **Re-audit (2026-11-26): the fix misses the boundary.** The text check counts the exponent's digits, but libmpdec's limit is on the adjusted exponent, so a literal whose exponent has 18 digits and whose adjusted exponent reaches 10^18 still crashes `Decimal(text)`. Five literals, each as `filter` `x < <literal>` over one table `t` (`k` `str` key, `x` `dec` scale 2), run as steps schema cases before any further change (`validate.py`; `Steps::from_json` then `Steps::check` in `tm-formats`):

  | Literal | Adjusted exponent | Python | Rust |
  | --- | --- | --- | --- |
  | `10e999999999999999999` | 10^18 | exit 1, `decimal.InvalidOperation` | `E6004` `/ops/0/where` |
  | `1e999999999999999999` | 10^18 - 1 | `E6004` `/ops/0/where` | `E6004` `/ops/0/where` |
  | `0e999999999999999999` | 10^18 - 1 (zero coefficient counts one digit) | `E6004` `/ops/0/where` | `E6004` `/ops/0/where` |
  | `0.1e-999999999999999999` | -10^18 | `E6004` `/ops/0/where` | `E6004` `/ops/0/where` |
  | `1e-999999999999999999` | -(10^18 - 1) | `E6004` `/ops/0/where` | `E6004` `/ops/0/where` |

  Only the first differs: `Decimal` holds an exponent down to its `MIN_ETINY`, far below -10^18, so the fourth passes it, and the first is the one combination the digit count lets through. Under the ruling, Python computes the adjusted exponent from the text with Python ints, as `tm-formats`' `adjusted_exponent` does, once the exponent has 18 digits or fewer, and calls `Decimal` only for a literal that passed; the bounds are unchanged.
- Applied 2026-11-26: `exprs.adjusted_exponent` and the check in `number_literal` before `Decimal(text)`, logged in `docs/decisions.md` under the D22 exception. The five probes rerun through `validate.py` are all `E6004` `/ops/0/where`, equal to Rust.
