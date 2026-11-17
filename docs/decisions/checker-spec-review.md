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

## Owner rulings on the design-round report

Timothy Stephens, 2026-11-17. The design round raised six open items and three findings on the exponent and column-scale rules.

35. The decimal context is fixed explicitly in both implementations: precision 34, Emax 999999, Emin -999999, rounding half-even. A result whose adjusted exponent exceeds Emax is GDA Overflow, E5002. Underflow follows GDA (subnormal, then zero) and is not an error, which is Python decimal's default behaviour. Lands in 11.6 and 11.18.2 (Domain, Precision), replacing "the context's Emax".

36. A numeric literal in an expression whose adjusted exponent is above Emax or below Emin is E6004 at validation, pointing at its where or expr member (ruling 27). Python constructs Decimal values exactly without applying the context, so the reference implementation must check this explicitly. Lands in 11.18.1.

37. On error, every row-level field of a case's expected.json is null: steps, columns, final_rows, final_rows_hash, and key_map_hash. Lands in item 92.

38. The claim page's steps begin with step 0, the cited input table before the first op, capped like every step. Every row of the final step comes from the key map. The rows the assertion names (row, left, right) and the cells in the column it names carry the emphasis. A capped final step shows its first limit rows plus every row the assertion names that lies beyond the cap, each labeled with its position (for example "row 4,812 of 20,000"). Removing "with neighbors" and truncated stands. Lands in 11.20, section 10's claims.evidence row, and item 55's result type.

39. A dec cell is E3102 only when writing it at the column's scale would change its value. Extra trailing zeros are not a change: a scale-3 column accepts 0.001, 0.0010, and 1.0e-3, and refuses 0.0011. Lands in 11.7 and item 47, replacing the E3102 sentence ruling 5 added.

40. Accepted as the builder chose: item 55 (run.rs) carries the per-step result type, so stored claim.checked bodies are unchanged; the claims.evidence edits in section 10, where the row lives; the edit to item 47; item 47's full case-insensitive nan and inf token list in place of ruling 12's short list; regenerating docs/spec/ inside design commits; and refusing a CSV exponent that needs more than 38 digits either side of the point.

## Owner rulings on the case writer's second list

Timothy Stephens, 2026-11-17. The case writer re-read the revised checker.md and found seven more gaps, numbered 32 to 38 in its list. This is the last case-writer pass on the specification; later findings go to checker-diffs.md.

41. (Writer's 32.) select removes columns and never reorders them: its output keeps the table's column order. The select description's {cols} is the op's column list exactly as written in the op, without the key columns that select keeps implicitly. Lands in 11.18.2 select and 11.18.4.

42. (Writer's 33.) Every evaluation error, E5002 included, points at the member holding the expression or aggregate that raised it (the expr, the where, or the aggregate entry), no finer. This generalizes ruling 27. Lands in 11.12 and 11.18.2.

43. (Writer's 34.) At table.spec, the scale a cell shows is its number of digits after the point once written positionally, trailing zeros included, so 1.50e-2 shows scale 4 and 0.0010 shows scale 4. The column's scale is the largest shown. Ruling 39 still lets a later version's trailing zeros fit. Lands in 11.7 and item 47.

44. (Writer's 35.) Not a conflict: "multiplication's scale is the sum of the operands' scales" describes the exact product, and General Decimal Arithmetic then rounds any result over 34 significant digits, which reduces its scale. State both in one sentence. Lands in 11.6.

45. (Writer's 36.) A literal written with an exponent is a positional dec before any arithmetic (ruling 32), so 1.5 * 1e3 is 1500.0. Python's Decimal('1e3') keeps exponent +3, so the reference implementation quantizes a literal to its positional scale before using it. Lands in 11.18.1.

46. (Writer's 37.) In a str column every token is text, so a cell reading nan is the string "nan". The nan and inf token rule applies only to int, dec, bool, date, and datetime columns, where such a token is E3102 unless it is in the missing list, and to float64 columns, where it is a value. Lands in 11.7 and item 47.

47. (Writer's 38.) Contradiction: the six functions (log, log2, log10, exp, mean, median) and "absorbed rather than tolerated" stand. The sentence in 11.6 saying "these four are the only places last-digit differences are tolerated" is removed.

## Rulings on the case writer's last pass

Timothy Stephens, 2026-11-17. Two rulings on the case writer's final pass (its findings 39 and 40), applied in the design before any Checker code.

48. (Writer's 39.) Converting a literal to its positional form (11.18.1, ruling 45) is exact and happens outside the decimal context; only the arithmetic that follows applies the context. The reference implementation must not quantize in the precision-34 context, which fails for 9.9e999999. Lands in 11.18.1.

49. (Writer's 40.) A key column written explicitly in a select's list appears in the description, because it was written; the exclusion covers only key columns the op does not list. Lands in 11.18.4.

## Owner rulings on the P4 design review

Timothy Stephens, 2026-11-17. From reviews/2026-11-16-p4-checker-design-decisions.md, findings M3 and the three further Rust-only behaviours. The reviewer drafted these; the owner changed 50.

50. In group's by columns and rank's group_by, every missing, nan, or inf value in a component falls in one group for that component, as SQL GROUP BY does, and that group's value in the output is missing, whatever its members held. This is grouping only: the = rule in expressions still never makes two missing values equal. Missing groups sort first (ruling 26). Lands in 11.18.2 group and rank.

51. When several members are equal under the = rule, min and max return the first such member in input order, as written (2 or 2.0, whichever came first). Lands in 11.18.2 group.

52. When the table key breaks a tie, a missing key component sorts first. Lands in 11.18.2 sort and Initial order.

53. A float64 cell whose text is a finite number that overflows a double (1e400) is E3102. Only the declared inf tokens produce infinity. Lands in 11.7 and item 47.

## Owner rulings on the case writer's fourth pass

Timothy Stephens, 2026-11-17.

54. (Writer's 41.) An E3102 about a data file cell has field null; its message names the row and the column. A CSV cell is not a JSON member, and E1001 and E1003 already take null for the same reason. Lands in 11.12 and 11.7.

55. (Writer's 42.) A row value in an assertion (row, left, or right) may be the typed value missing, and it matches a key component that is missing under grouping identity (ruling 50), not under the = rule, so a claim can name the missing group: "312 genes have no annotated pathway". 11.9's "of its column's type" becomes "of its column's type, or missing". Lands in 11.9 and 11.18.3.
