# checker.md sign-off: the case writer's 31 findings, triaged

Draft of `docs/decisions/checker-spec-review.md` for the owner to sign. Date: 2026-11-13. Source: the case writer's AMBIGUITIES.md, written from `checker.md` alone with no implementation present. 116 cases were written; 88 holds, 19 fails, 9 error.

Verdict column: **adopt** = the writer's reading becomes design text; **overrule** = the design says otherwise and the writer's reading is replaced; **contradiction** = two design sentences disagree and one is chosen. Case impact says whether any written case must change.

| # | What the spec left open | Ruling | Where it lands | Case impact |
| --- | --- | --- | --- | --- |
| 1 | `expected.json` has no stated shape; the two hashes are not listed | adopt: `{verdict, steps:[{rows,description}], columns, final_rows, final_rows_hash, key_map_hash, error}`; `error` is `{code,field}` or null; on error the row-level fields are null | item 92 | none |
| 2 | eval `expected.json` shape | adopt: `{verdict, error}` | item 92 | none |
| 3 | nothing binds a steps table name to a file in `tables/` | adopt: the name is the file stem; the handle in the case is a placeholder the harness ignores | item 92 | none |
| 4 | a table's order before its first op | adopt: key order (11.10 per column, composite left to right) | 11.18.2 new row "Initial order" | none |
| 5 | 11.6 "scale as written" versus DuckDB `DECIMAL(38,s)` fixed per column | **contradiction**: the column's spec `scale` wins; a parsed value is written at column scale (`1.5` in a scale-3 column is `1.500`). Values leave DuckDB at column scale regardless, so "as written" could only ever hold before the first relational step | 11.6 `dec` row, 11.7 | none (every case has uniform scale) |
| 6 | `select` template `{cols}` order | adopt: as written in the op | 11.18.4 | none |
| 7 | sort template separator for three or more columns | adopt: `, then ` repeated | 11.18.4 | none |
| 8 | group and join templates ending in `, ...` | adopt: the general list rule | 11.18.4 | none |
| 9 | `not (a > b)` renders with or without parentheses | adopt: dropped; the grammar makes `not` bind looser than a comparison so they are redundant | 11.18.4 `{expr}` | none |
| 10 | two missing rows under `rank`: tie or split | adopt: tie, as SQL `RANK` | 11.18.2 `rank` | none |
| 11 | `rank`'s missing rule and arithmetic for `nan` and `inf` | adopt: `inf` is missing for every operator, exactly as `nan`; `rank` places `nan` and `inf` by its `missing` rule, as `sort` does | 11.6 last row, 11.18.2 Missing | none |
| 12 | which text tokens are NaN and Inf, in which column types | adopt what P3 item 47 already says: `NaN`, `Inf`, `-Inf`, float64 columns only; copy the sentence into 11.7 so `checker.md` carries it | 11.7 | none |
| 13 | do `mean` and `median` ignore missing | adopt: yes, and give `missing` when every member is missing, as `sum` does | 11.18.2 `group` | none |
| 14 | `member_keys_hash` sort: component-wise by value or by canonical text | **overrule**: the literal reading. 11.10 is binary string order, so tuples sort by their canonical JSON text. It is a hash input; only determinism matters | 11.18.2 `group` | none (writer chose keys where both agree) |
| 15 | JSON form of a key tuple | adopt: one JSON list of typed values in key column order | 11.18.2 Key mapping | none |
| 16 | "decimal overflow beyond 34 digits is E5002" versus GDA rounding at 34 digits and `int` of arbitrary size | **overrule**: General Decimal Arithmetic rounds at the context precision; it does not overflow there. `dec` results round to 34 significant digits half-even. `int` results are arbitrary size and never overflow. `E5002` is GDA's Overflow condition only (exponent past `Emax`), which Python `decimal` raises identically | 11.18.2 Domain and Precision | **`derive_overflow_error` must be re-derived**: its result rounds, verdict is not `error` |
| 17 | written form of an `int` widened to `dec` by `coalesce` | adopt: scale 0, written `2` | 11.18.2 Types | none |
| 18 | division "exact when it terminates within 34 digits": which digits; `4 / 4` | adopt: GDA at precision 34, which is Python `decimal` with `prec=34`; `4 / 4` is `1`. Reword to say so | 11.6 `dec` row | none |
| 19 | 15-digit rounding of the exact binary value or of its shortest text | adopt: the exact binary value | 11.6 `dec` row | none |
| 20 | `tol` defaults to zero, but `tol` on a non-number is `E6004` | **contradiction**: a *present* `tol`, zero or not, on a non-numeric cell is `E6004`; the default applies only when `tol` is absent | 11.9, 11.18.3 | none |
| 21 | `count` with `between` | adopt: allowed; `expected` is two ints | 11.9 | none |
| 22 | `expected.json` on error; whether earlier steps keep counts | adopt: row-level fields null, `error` set; step counts are not reported on error | item 92 | none |
| 23 | `rank` assertion: 11.9 "the row's rank in the final table" versus 11.18.3 "1-based position" | **contradiction**: position. It must work after a plain `sort` with no `rank` op, and after a `rank` op with ties the two differ; position is the one a reader can see | 11.9 | none (writer took position) |
| 24 | order of added right columns; fill of the right key on unmatched `left` rows | adopt: right table's column order; `missing` in every right column including its key | 11.18.2 `join` | none |
| 25 | `join.on` with a non-key left column | adopt: allowed; only the right key must be covered | 11.8 `join`, 11.18.2 | none |
| 26 | group order when a `by` value is missing | adopt: groups with a missing component sort first, component by component | 11.18.2 `group` | none |
| 27 | no finer `E6004` pointer inside an expression | adopt: the pointer names the `where` or `expr` member; no finer | 11.12, 11.18.2 | none |
| 28 | column type of `member_keys_hash` | adopt: `str` | 11.18.2 `group` | none |
| 29 | float64 columns produce `dec` typed values | adopt, already implied by 11.6; state it in 11.7's type list | 11.7 | none |
| 30 | verdict vocabulary never listed in one place | adopt: the Checker produces `holds`, `fails`, `error`; `not_checkable` is an evidence outcome (7.3), never a Checker verdict | 11.18.3 | none |
| 31 | `between` on dates | adopt: allowed, chronological; `tol` on a date is `E6004` as already stated | 11.9 | none |

## What this says about the specification

Thirty-one gaps in a section meant to be implementable from outside. Four are contradictions between sentences I wrote in different fix rounds (5, 16, 20, 23). The rest are silences a careful implementer would have resolved the same way the writer did, which is why only one written case changes. The specification was not wrong so much as not finished, and the independent reading is what finished it.

## Sign-off procedure

1. Read `docs/spec/checker.md` end to end, once, with this table beside it. The question is not "is each ruling right" but "with these 31 fixes in, could someone implement from this file alone". If the answer is no, say what is still missing.
2. Mark any ruling you overrule. "Defaults" means every row stands as written.
3. The design edits land in one design-first commit on `p4-checker`; `spec-drift.sh` regenerates `checker.md`; the case writer re-derives `derive_overflow_error` from the new text and nothing else.
4. This file, with your marks and date, becomes `docs/decisions/checker-spec-review.md`.

## Signed

Timothy Stephens, 2026-11-16. All 31 rulings stand as written. Three owner rulings added at sign-off:

32. Numbers may be written with an exponent wherever a person or a tool writes one: CSV cells in dec and float64 columns, and expression literals. They are normalized to the canonical positional form on input. Typed values stored and hashed stay positional only. Descriptions keep literals as written. The static view may display values in scientific notation; tm-data carries the canonical form. Lands in 11.6, 11.7, 11.18.1, and 11.20. P1 case conformance/formats/expr/err_exponent becomes valid and is regenerated, citing this ruling. conformance/formats/typed_values/dec_exponent stays invalid.

33. Highlight is driven by citation, not assertion. The final step's rows are highlighted from the key map, any number of them. Within those rows the cells in the column or columns the assertion names are emphasized, plus any columns a cells citation lists. Lands in 11.20 and section 9's claims.evidence row.

34. The claim page shows every step's output in step order, each collapsible, with highlighting on the final step only. claims.evidence returns each step's table capped at limit with its true row count. tm-checker's result type carries per-step tables and the emphasized columns, so P7 does not rebuild it. Lands in 11.20, section 9's claims.evidence row, and the tm-checker item that defines the result type.
