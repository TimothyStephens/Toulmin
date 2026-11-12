<!-- design-sections: 11.6-11.10, 11.18, item 92 -->
## 11.6 Typed values

One encoding everywhere (events, steps, assertions, Arrow conversion):

| Type | Encoding | Notes |
| --- | --- | --- |
| `int` | `{"t":"int","v":"42"}` | Decimal string; arbitrary size |
| `dec` | `{"t":"dec","v":"0.00310"}` | Decimal string, always positional notation, never an exponent. Parsed values keep the scale as written, trailing zeros included. Computed values follow the General Decimal Arithmetic specification (IEEE 754-2008 decimal) at 34 significant digits, rounding half-even, so multiplication's scale is the sum of the operands' scales (`1.10 * 2` is `2.20`), addition and subtraction take the larger scale, and division is exact when it terminates within 34 digits and otherwise rounds (`10 / 4` is `2.5`). Negative zero is written without its sign, scale kept (`-0.00` is `0.00`). A value that reaches this encoding from an IEEE double (`log`, `exp`, `mean`, `median`) is first rounded to 15 significant digits, then written with the shortest digits that round-trip at that precision, in positional notation |
| `str` | `{"t":"str","v":"..."}` | NFC-normalized |
| `bool` | `{"t":"bool","v":true}` | |
| `list` | `{"t":"list","v":[typed values]}` | Settings only in gate A; never a column value |
| `date`, `datetime` | `{"t":"date","v":"2026-11-09"}` | RFC 3339 |
| `missing` | `{"t":"missing"}` | Null, NA tokens, and anything the parse spec maps to missing |
| `nan`, `+inf`, `-inf` | `{"t":"nan"}`, `{"t":"inf","s":"+"}` | Only when the parse spec maps them to a value instead of missing; treated as missing by filters and placed by the sort's missing rule |

Lexical forms, checked wherever a typed value is read (`E1004` with `field` at the offending member; `E1002` inside an envelope line): an object has exactly the keys shown for its `t`. `int` `v` matches `-?(0|[1-9][0-9]*)` and `dec` `v` matches `-?(0|[1-9][0-9]*)(\.[0-9]+)?`; in both, a minus sign before digits that are all zero is refused. `str` `v` is normalized to NFC when read, never refused: a reader replaces a `v` that is not NFC with its NFC form, and a writer emits NFC, so a value that was read and written again is NFC. `date` `v` is `YYYY-MM-DD`, a valid proleptic Gregorian date in years 0001 to 9999; `datetime` `v` is `YYYY-MM-DDTHH:MM:SS`, then optionally `.` and one to nine fraction digits with no trailing zero, then `Z` (UTC only; a whole second carries no fraction). `list` elements are typed values other than `list`. `s` of `inf` is `+` or `-`.

Floats from a table are parsed as decimals from their text; a parse spec may declare a column `float64`, in which case the value is the shortest round-trip decimal string of the IEEE value, in positional notation, with either zero written `0`. Arithmetic in `derive` is decimal with a stated precision (34 digits), except `log` and `exp`, which are IEEE double; `mean` and `median` in `group` are also IEEE double in DuckDB. These four are the only places last-digit differences are tolerated. Decimal columns map to DuckDB `DECIMAL(38, s)` with `s` fixed per column when the parse spec is set as the larger of the draft's scale and the largest scale seen (item 47); a value that does not fit at that scale is a parse failure (`E3102`), and an arithmetic overflow in `derive` gives verdict `error` with `E5002`. The differential suite runs the same cases on two CPU generations, since DuckDB selects code paths by CPU feature at runtime.

## 11.7 Parse spec

`{"spec":1,"format":"csv|tsv|parquet","delimiter","header":true,"columns":[{"name","type","missing":["NA",""]}],"key":["gene_id"],"nan":"missing|value","inf":"missing|value","encoding":"utf-8"}`. `type` is one of `int`, `dec`, `str`, `bool`, `date`, `datetime`, or `float64` (item 47's mapping); `missing` is the list of tokens read as `missing` in that column; a `dec` column also carries `scale`, a JSON integer from 0 to 38 that `table.spec` records as the larger of the draft's and the largest scale seen (item 47), and no other type carries it. Every key shown is required: `delimiter` is a one-character string and `header` a boolean for `csv` and `tsv`, and both are `null` for `parquet`. Column names are non-empty and distinct; `key` is a non-empty list of distinct column names. A key name the columns list does not have, or a column the file's header does not have, is `E3103`; any other schema violation is `E3105`; `field` points at the offending member. `utf-8` is the only encoding accepted in V1; another value is refused with `E3104`. Its fingerprint is the ID of its canonical JSON. Key uniqueness is checked at `table.spec` and again when a new version arrives.

## 11.8 Query steps

`{"steps":1,"tables":{"de":"F7@v2"},"from":"de","ops":[...]}`. `from` names the table the pipeline starts from and is required even when `tables` holds one entry; a `from` that names no entry in `tables` is `E6002`. A claim may carry an assertion with `steps` null: the final table is then the rows its `cells` citation names, in key order, with no pipeline and no SQL, which is how a single-cell `value` assertion is written. Operations and their versions:

| Op (version) | Required fields |
| --- | --- |
| `filter` (1) | `where`: expression; missing never matches |
| `select` (1) | `columns` |
| `sort` (1) | `by`: list of `{column, dir}` with `dir` one of `asc` or `desc`; `missing`: `first` or `last`; the table key is appended as the last tie-breaker automatically and recorded in the SQL |
| `take` (1) | `n`; requires a preceding `sort` or `rank` (total order) |
| `rank` (1) | `by`, `dir`, `ties`: `min`, `max`, `dense`, or `ordinal` (ordinal uses the key as tie-breaker), `missing`: `first`, `last`, or `exclude`, `scope`: `all` or `group`, `group_by`: column list, required when `scope` is `group` |
| `derive` (1) | `as`, `expr` from a closed grammar: arithmetic, comparison, `abs`, `log`, `log2`, `log10`, `exp`, `coalesce`, string equality; no `random()`, `now()`, or user functions |
| `group` (1) | `by`, `aggregates`: list of `{as, fn in count,sum,mean,min,max,median, column}`; output ordered by group keys; each row carries member count and member-key fingerprint |
| `join` (1) | `right`: table name, `on`: `[{"left":"col","right":"col"}, ...]`, `kind`: `inner` or `left`; both sides must be keyed and the pairs must cover the right key |

Each op is an object `{"op":<name>,"v":<version>, ...}` holding exactly `op`, `v`, and its required fields (and `group_by` only when `scope` is `group`). `filter.where` and `derive.expr` are expression strings (11.18.1); `select.columns` and `rank.group_by` are non-empty lists of column names; `rank.by` is one column name; `sort.by` is a non-empty list of `{"column","dir"}` objects; `take.n` is a JSON integer of at least 0; `group.by` is a non-empty list of column names and `group.aggregates` a non-empty list; `join.on` is a non-empty list. `tables` is a non-empty object from table name to version handle, and `ops` is a list, possibly empty.

Validation codes, each with `field` the JSON Pointer (11.12) of the offending member, or of where an absent member belongs: `E6002` for a required member that is absent, a required list that is empty, a choice not among its listed values, an unknown `op` or op version (in a stored body a version above the known one makes the event opaque instead, 11.16), a key the op does not define, a `take` with no earlier `sort` or `rank`, a `join` whose `right` names no entry of `tables` or whose `on` pairs do not cover the right table's key, and a step that would add a column whose name is already present; `E6004` for a member of the wrong JSON type or form (a string that is not a version handle, a negative number where a non-negative one is required) and for an operand type error under 11.18.2; `E6005` for a column name the table does not have at that step; `E6006` for an expression that does not parse under 11.18.1. Checks that need no parse spec are made wherever steps are read; `E6005`, the operand types, and the join cover are checked against the cited tables' parse specs before anything runs.

Each op's semantics are frozen at its version; a change is a new version number, and the old one stays runnable.

## 11.9 Assertion schema

`{"assert":1,"kind":...}`:

| Kind | Fields | Meaning |
| --- | --- | --- |
| `value` | `row` (key values), `column`, `op`, `expected`, `tol` | The cell compares to `expected` |
| `rank` | `row`, `expected`, `tol` | The row's rank in the final table equals `expected` |
| `count` | `op`, `expected` | Final row count compares |
| `compare_rows` | `left` row and column, `right` row and column, `op`, `tol` | Two cells compare |
| `exists` | `row` | The row is present in the final table |

`op` in {`eq`, `ne`, `lt`, `le`, `gt`, `ge`, `between`}. Numeric comparison rule: both sides are decimals; `tol` is `{"abs":"0","rel":"0"}` by default and must be stated to be non-zero; `eq` with tolerance means `|a - b| <= abs + rel * |b|`; a missing value never satisfies any comparison and yields `fails`, not `not checkable`.

Shapes: `row` is a list of typed values, one per key column of the final table in its key order (11.18.2 gives the output key of each op); `column` is a column name; `left` and `right` are `{"row","column"}` objects; `expected` is a typed value, an `int` for `rank` and `count`, and for `between` a two-element JSON list `[lo, hi]` of typed values; `expected` is never `missing`, `nan`, `inf`, or `list`. `tol` is optional and, when present, exactly `{"abs","rel"}`, each a non-negative decimal in the 11.6 `dec` `v` form; `count` and `exists` take no `tol`. `rank` takes no `op` and compares as `eq`; `compare_rows` does not take `between`. The keys are exactly those listed per kind plus `assert` and `kind`. Validation codes follow 11.8: `E6002` for an absent, unknown, or not-allowed member or an empty `row`, `E6004` for a wrong JSON type or form or a value whose type the comparison does not allow. An operator that the type of `expected` does not allow is `E6004` at `/op`, and a `tol` it does not allow is `E6004` at `/tol`. When the final table's schema is known (11.18.2 derives it from the cited specs and the steps), the assertion is checked against it before anything runs: `row` has one value per key column, each of its column's type (`int` and `dec` interchangeable), else `E6004` at `/row/<i>` (under `/left` or `/right` for `compare_rows`); `column` names a column, else `E6005` at `/column`; `expected` is of a type that compares with the column's under 11.18.3, else `E6004` at `/expected`; for `compare_rows` the two columns' types compare, else `E6004` at `/right/column`. At evaluation (11.18.3), a cell whose type does not compare with `expected` is `E6004` at `/expected`, or at `/right` for `compare_rows`, and a `tol` on a cell that is not a number is `E6004` at `/tol`; found at a re-check (a later version whose column type changed), that is recorded as verdict `error` with `E6004`, not refused (7.3).

## 11.10 Text sort order

Binary order of NFC-normalized UTF-8 bytes, which is DuckDB's default binary collation. No locale collation anywhere. The SQL sets `ORDER BY ... COLLATE BINARY` explicitly.

## 11.18 Checker semantics

### 11.18.1 Expression grammar (`filter.where`, `derive.expr`)

```
expr     := or
or       := and ( "or" and )*
and      := not ( "and" not )*
not      := "not" not | cmp
cmp      := add ( ( "=" | "!=" | "<" | "<=" | ">" | ">=" ) add )?
add      := mul ( ( "+" | "-" ) mul )*
mul      := unary ( ( "*" | "/" ) unary )*
unary    := "-" unary | primary
primary  := literal | column | call | "(" expr ")"
call     := ( "abs" | "log" | "log2" | "log10" | "exp" | "coalesce" ) "(" expr ( "," expr )* ")"
literal  := INT | DEC | STRING | "true" | "false" | "missing"
column   := IDENT | "`" any-but-backtick+ "`"
INT      := DIGIT+                            (typed value int; unsigned)
DEC      := DIGIT+ "." DIGIT+                 (typed value dec; unsigned, no exponent form)
STRING   := '"' ( any-but-quote-or-backslash | "\\" '"' | "\\" "\\" )* '"'
IDENT    := [A-Za-z_][A-Za-z0-9_]*
```

Whitespace between tokens is ignored. Literals carry no sign: `-` is always an operator, unary or binary, so `a -5` is `a - 5` and a negative constant is unary minus applied to an unsigned literal. `coalesce` takes two or more arguments; every other call takes one. The words `and`, `or`, `not`, `true`, `false`, and `missing` are reserved, lower case only, and need backticks as column names. An `IDENT` followed by `(` is a call, and a call to a name the `call` rule does not list is a syntax error; any other `IDENT` is a column, so a column may be named `log`. A call with the wrong number of arguments, an unterminated string or backtick name, a backslash in a string followed by anything but `"` or `\`, an empty backtick name, a chained comparison (`a < b < c`), and any other token sequence the grammar does not derive are syntax errors, `E6006`.

### 11.18.2 Evaluation rules

| Rule | Statement |
| --- | --- |
| Where | `filter.where` and `derive.expr` are evaluated in Rust over the Arrow batch with `bigdecimal` (the reference checker uses Python `decimal` with the same context); the relational operations (`sort`, `take`, `rank`, `group`, `join`, `select`) run in DuckDB over the result. The stored SQL covers the relational part and names the derived columns as inputs. |
| Types | Operands are typed values (11.6); a `float64` column is `dec`. `int` combined with `dec` gives `dec`. `+`, `-`, and `*` on two `int` give `int`; `/` always gives `dec`; unary `-` and `abs` keep `int` or `dec`; `log`, `log2`, `log10`, and `exp` take `int` or `dec` and give `dec`; `coalesce` gives its arguments' common type, `int` and `dec` widening to `dec`, and its arguments otherwise share one type. Arithmetic on any other type is `E6004`. `=` and `!=` take two numbers, two `str`, two `bool`, two `date`, or two `datetime`; `<`, `<=`, `>`, and `>=` take the same pairs except `bool`; a `date` and a `datetime` do not compare. `and`, `or`, and `not` take `bool`. The literal `missing` has no type and fits any operand position; a `derive` whose result has no type is `E6004`. Comparisons give `bool`. |
| Precision | Decimal arithmetic follows the General Decimal Arithmetic specification (IEEE 754-2008 decimal) with a context of 34 significant digits and half-even rounding: `bigdecimal` on the Rust side, Python's `decimal` on the reference side, both named in 11.6. Results of `log`, `log2`, `log10`, `exp`, `mean`, and `median` are rounded to 15 significant digits before they become `dec` values (see the `log` family row). |
| Missing | Any arithmetic or comparison with a `missing` operand is `missing`, and `nan` is `missing` for every operator. `and`, `or`, and `not` are SQL three-valued: `true or missing` is `true`, `false and missing` is `false`, `false or missing` and `true and missing` are `missing`, and `not missing` is `missing`. A `filter` keeps a row only when `where` is `true`; `missing` and `false` drop it. A `where` that evaluates to anything but a boolean or `missing` is `E6004`. |
| Domain | Division by zero, `log` of a value at or below zero, and `exp` overflow give `missing`, not an error. Decimal overflow beyond 34 digits gives verdict `error` with `E5002`. |
| Strings | `=` and `!=` compare NFC-normalized bytes. `<`, `<=`, `>`, `>=` on strings use 11.10 binary order. Comparing a string with a number, bool, or date is a validation error `E6004` type mismatch, raised before DuckDB runs. |
| Dates | `date` and `datetime` compare chronologically; arithmetic on them is `E6004`. |
| `log` family | `log` is the natural logarithm, translated to DuckDB `ln` where DuckDB sees it. `log`, `log2`, `log10`, and `exp` take and return `dec` through IEEE double, as do `mean` and `median`. These six are the only places two conforming implementations may differ, because the Rust side takes them from the `libm` it is linked against (musl's in the static build, the host's in the glibc fallback) and the reference checker takes them from CPython's, which can differ in the last bit on one machine. The difference is absorbed rather than tolerated: every result of these six is rounded to 15 significant digits before it becomes a `dec`, which is below the 16 to 17 digits at which a one-unit-in-the-last-place difference appears, so `final_rows_hash` is comparable for every pipeline and there is one comparison rule. A claim therefore cannot assert the 16th significant digit of a logarithm or a mean. |
| `sort` | `by` entries apply in order; `missing` rows go first or last per `missing`; the table key is appended ascending as the final tie-breaker. |
| `take` | Keeps the first `n` rows of the current total order; requires a preceding `sort` or `rank`. |
| `rank` | `scope: all` ranks the whole table; `scope: group` requires `group_by` (a non-empty column list) and ranks within each group. `missing: first` or `last` assigns ranks as if sorted that way; `missing: exclude` drops rows whose `by` value is missing. `ties: min`, `max`, and `dense` follow SQL `RANK`, inverse `RANK`, and `DENSE_RANK`; `ordinal` is `ROW_NUMBER` ordered by `by` then the key. The rank column is named `rank` and the table order becomes rank ascending then key; a table that already has a `rank` column must `select` it away first, since adding a present name is `E6002` (11.8). |
| `group` | Output rows are one per distinct `by` tuple, ordered by the tuple in sort order. Output columns: the `by` columns, each aggregate under its `as`, `member_count` (int), and `member_keys_hash` (`sha256:` of the canonical JSON list of the members' key tuples sorted in 11.10 order). `count` counts every member, missing included; `sum`, `min`, and `max` ignore missing and give `missing` when every member is missing; `mean` and `median` are DuckDB `DOUBLE` rounded to 15 significant digits and converted to `dec` per 11.6. Types: `count` gives `int`; `sum` takes `int` or `dec` and keeps it; `min` and `max` take any type but `bool` and keep it; `mean` and `median` take `int` or `dec` and give `dec`; any other column type is `E6004`. The output key is the `by` tuple. |
| `join` | `on` pairs left columns with right columns; both sides must be keyed and the pairs must cover the right key. Every right column, its key columns included, is added with the prefix `<right>.`. Each `on` pair must compare under the `=` rule of the Types row (`E6004`). `inner` drops unmatched left rows; `left` keeps them with `missing`. The output key is the left key. |
| `select` | Keeps the listed columns and the key columns. |
| Key mapping | Each final row carries the input key tuple(s) it derives from: the left key for joins, the `by` tuple for groups, the row key otherwise. `key_map_hash` is `sha256:` of the canonical JSON list of those tuples in final-row order. |
| Result hash | `final_rows_hash` is `sha256:` of the canonical JSON list of the final rows, in final order, each row an object of column name to typed value, columns in the order the last step produced them. |

### 11.18.3 Assertion rules

| Kind | Rule |
| --- | --- |
| `value` | Locate the final row by `row` (key values); absent row is `fails`. Compare the cell with `expected` under `op` and `tol`. |
| `rank` | The row's 1-based position in the final order; absent row is `fails`. Compared with `expected` under `tol` as an `eq`. |
| `count` | Final row count compared with `expected` under `op`. |
| `compare_rows` | Both cells located as for `value`; either absent is `fails`. |
| `exists` | `holds` if the row is present, `fails` otherwise. |
| `op` on strings | `eq` and `ne` only, by NFC bytes; other operators and `tol` are `E6004`. |
| `op` on booleans | `eq` and `ne` only; other operators and `tol` are `E6004`. |
| `op` on dates | Chronological; `tol` is `E6004`; a `date` and a `datetime` do not compare (`E6004`). |
| Mixed types | `int` and `dec` compare as decimals; any other pair of different types is `E6004`. |
| Tolerance | With `m = abs + rel * |b|`, where `b` is `expected` or, for `compare_rows`, the right cell, computed exactly without rounding, and `m = 0` with no `tol`: `eq` holds when `|a - b| <= m`; `ne` when `|a - b| > m`; `lt` when `a < b + m`; `le` when `a <= b + m`; `gt` when `a > b - m`; `ge` when `a >= b - m`. |
| `between` | `expected` is `[lo, hi]`, inclusive; holds when `lo - (abs + rel * |lo|) <= a <= hi + (abs + rel * |hi|)`. The bounds are not reordered, so `lo > hi` holds for nothing beyond the tolerance. |
| Missing | A `missing`, `nan`, or `inf` cell never satisfies any comparison and gives `fails` (11.9). |

### 11.18.4 Description templates

One sentence per step, produced by the Checker and reproduced byte for byte by the reference checker. Placeholder renderings: `{n}`, `{m}`, and `{k}` are integers with `,` as the thousands separator; `{expr}` is the parsed expression re-serialized from the parse tree, never echoed from the author's text: one space around every binary operator and around `and` and `or`, one space after `not`, no space after unary `-`, no space inside parentheses, one space after each comma in a call, column names as written with backticks kept only where the grammar requires them, literals as written, and parentheses emitted only where precedence requires them; `{cols}` and every other list is comma-separated with `, and ` before the last item when there are three or more and ` and ` for two; `{dir}` is `ascending` or `descending`; `{missing}` is `placed first`, `placed last`, or `excluded`; `{ties}` is the `ties` value as written (`min`, `max`, `dense`, `ordinal`); `{scope}` is `all rows` or `each {group_by}`; `{key}` is the table key column list; `{fn}` is the aggregate name as written; `{right}` is the right table's name as registered.

| Op | Template |
| --- | --- |
| `filter` | `Keep rows where {expr} (missing values excluded); {n} of {m} rows remain.` |
| `select` | `Keep columns {cols}; {n} rows.` |
| `sort` | `Sort by {col1} {dir1}, then {col2} {dir2} (missing values {missing}; ties broken by {key}); {n} rows.` |
| `take` | `Take the first {k} rows; {n} rows remain.` |
| `rank` | `Rank by {col} {dir} within {scope} (ties: {ties}; missing values {missing}); {n} rows.` where `{scope}` is `all rows` or `each {group_by}` |
| `derive` | `Add column {as} = {expr}; {n} rows.` |
| `group` | `Group by {cols} and compute {as1} = {fn1}({col1}), ...; {n} groups from {m} rows.` |
| `join` | `{Inner\|Left} join with {right} on {left_col} = {right_col}, ...; {n} rows.` |

92. `conformance/checker/cases/<name>/` with `tables/*.csv`, `specs/*.json`, `steps.json`, `assertion.json`, `expected.json` (per-step counts, final rows, descriptions, verdict). At least 60 hand-checked cases: each op; each tie rule; each missing placement; NaN and Inf; composite keys; each join kind; group with each aggregate; each assertion kind and operator; each 11.18.2 domain rule. Also `conformance/checker/assertions/eval/<name>/` with `case.json` (one assertion and what the Checker located, `observed`, in the case format `crates/tm-formats/README.md` gives), `observed.p1.json` (the 11.18.3 outcome or error the P1 Rust implementation gave, kept for comparison and never read as the expectation), and `expected.json`, written at P4 by the separate session from `checker.md` and `case.json` only; the P4 conformance test compares both implementations with `expected.json`. Rationale: the anchor for the Python checker.
