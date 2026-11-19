# Reference checker

An independent Python implementation of the Toulmin Checker written from
`checker.md` alone, for differential testing against the Rust checker.
Plain Python 3 (3.11 or later), standard library only, plus the two
reference format modules in `reference-formats/` (`canonical_json.py`,
`ids.py`), which it imports and does not reimplement. Both scripts run
under `python3 -I`: they put their own directory and `reference-formats/`
on `sys.path` themselves.

## Running a case

```
python3 -I reference/checker/refcheck.py conformance/checker/cases/<name>
python3 -I reference/checker/refcheck.py conformance/checker/cases/<name> --check
python3 -I reference/checker/refcheck.py conformance/checker/assertions/eval/<name> --check
```

Without `--check` the script prints one JSON object in the item 92 shape:
`{verdict, steps:[{rows, description}], columns, final_rows,
final_rows_hash, key_map_hash, error}` for a checker case (the row-level
fields are null and `error` is `{code, field}` on error), or
`{verdict, error}` for an eval case (a directory holding `case.json`).
With `--check` it compares that object with the directory's
`expected.json`, exits 0 when equal and 1 otherwise, and prints the
differing fields as JSON Pointers.

Every directory under both case trees:

```
for d in conformance/checker/cases/*/ conformance/checker/assertions/eval/*/; do
  python3 -I reference/checker/refcheck.py "$d" --check
done
```

## Validating steps and assertion documents

```
python3 -I reference/checker/validate.py conformance/formats-steps/invalid/*.json
python3 -I reference/checker/validate.py --check conformance/formats-assertions/schema/*.json
```

Each file is a schema case (`{"description", "input", ..., "expect"}`).
The input is validated against 11.8 (steps; against the file's `specs`
when present) or 11.9 (assertion; against the file's `table` schema when
present). Per file the script prints `ok` or `<code> <JSON Pointer>`.
With `--check` it also compares with the file's `expect` and exits 1 on
any mismatch.

## Modules

| Module | Covers |
| --- | --- |
| `typed_values.py` | 11.6 typed values, lexical checks (E1004), the decimal context, equality, grouping identity, order |
| `exprs.py` | 11.18.1 grammar, static types and evaluation (11.18.2), `{expr}` re-serialization (11.18.4) |
| `tables.py` | 11.7 parse specs and CSV/TSV cells |
| `ops.py` | 11.8 validation (shape, then against specs) and 11.18.2 execution of every op |
| `assertion.py` | 11.9 schema and 11.18.3 evaluation |
| `describe.py` | 11.18.4 description templates |
| `errors.py` | the `CheckError(code, message, field)` exception |

## Decimal context

All `dec` arithmetic (`+`, `-`, `*`, `/` in `derive`, `sum` in `group`)
runs in one `decimal.Context`:

```
precision 34, Emax 999999, Emin -999999, rounding ROUND_HALF_EVEN,
traps: Overflow only (reported as E5002 with verdict error)
```

Underflow, Subnormal, Inexact, Rounded and Clamped are not trapped, so
underflow gives a subnormal result and then zero, as 11.6 states. Division
by zero is checked before dividing and gives `missing`.

Conversions that 11.18.1 says are exact run outside that context, in a
second `Context` with `prec=decimal.MAX_PREC`: a literal or cell with an
exponent is made positional by appending zeros to its digit tuple (no
context at all), a cell is quantized to its column's scale with the exact
context, and the tolerance margin `abs + rel * |b|` and the differences it
is compared with are computed in the exact context. Nothing is quantized in
the precision-34 context.

`log`, `log2`, `log10`, `exp` use `math`; `mean` and `median` are computed
on IEEE doubles (plain summation in member order, then division; median by
linear interpolation between the two middle values). Each result is
rounded to 15 significant digits with `format(x, ".15g")`, which rounds
the double's exact binary value, trailing zeros are dropped, and the result
is written positionally.

## Readings taken where checker.md is silent

The numbered list in the implementation report; kept here so the code and
its assumptions stay together. Each item names the reading taken.

1. Version handle form (E6004 "a string that is not a version handle"): `<text without @ or whitespace>@v<positive integer>`.
2. Check order inside one op (shape phase): `op`, `v`, required members present in 11.8 order, rank's scope/group_by rule, undefined members, then member values.
3. Spec-phase order inside `derive`: `as` already present (E6002 `/ops/i/as`) before the expression's types.
4. `rank` adding a `rank` column already present: E6002 at `/ops/i`. A `group` aggregate `as` that repeats a `by` column, another `as`, `member_count` or `member_keys_hash`: E6002 at `/ops/i/aggregates/j/as`. A `join` adding a prefixed column already present: E6002 at `/ops/i/right`.
5. Static type of arithmetic with the `missing` literal on one side: the other operand's type (`/` always `dec`); `missing + missing`, `-missing`, `abs(missing)` and `coalesce(missing, missing)` have no type.
6. Unary minus and `abs` on a `dec` are exact (no rounding at 34 digits).
7. `mean`: plain double summation in member input order, divided by the count; `median`: linear interpolation between the two middle doubles; both ignore missing, nan and inf members.
8. `log` family on an int too large for a double: treated as infinity, result missing.
9. `bool` cells: `true` and `false` only, lower case.
10. `float64` cell text: `[+-]?(\d+(\.\d*)?|\.\d+)([eE][+-]?\d+)?`; nan/inf tokens matched in any letter case.
11. Run order: steps shape, steps against specs, assertion shape, assertion against schema, then table files are parsed (E3102, E3101, E3103), ops run, assertion evaluated.
12. Duplicate key in a table file: E3101, `field` null.
13. `and`/`or` evaluate both operands, so an E5002 on either side is raised.
14. `{expr}`: a right operand of equal precedence keeps its parentheses (`a - (b - c)`, `a and (b and c)`), the left drops them.
15. `compare_rows` schema order: left row, left column, right row, right column, type comparability (`/right/column`), op for the class (`/op`), tol (`/tol`). At evaluation: type comparability (`/expected` or `/right`) before tol.
16. `between` with `str` or `bool` bounds: E6004 at `/op`; `lo` and `hi` of different comparison classes: E6004 at `/expected`.
17. A `list` typed value in `row` or `expected`: E6004 (wrong type), not E1004; nan or inf in `row`: E6004 at `/row/i`.
18. A `tables` entry with no spec in the case: E6002 at `/tables/<name>`.
19. `take` accepts any earlier `sort` or `rank`, not only the immediately preceding op.
20. A `row` component `missing` matches a key component that is missing, nan or inf.
21. CSV cells are located by header name; a header lacking a spec column is E3103 at `/columns/i`; blank lines are skipped; a datetime second is 00 to 59.
22. Eval case `observed` shapes: `{"cell"}`, `{"position"}`, `{"count"}`, `{"left","right"}`, `{"present"}`, as the case files show.
