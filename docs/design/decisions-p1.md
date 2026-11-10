# Twelve decisions to close before P1

Date: 2026-11-10. These are the items from `implementation_readiness_report.md` (session 2, round 1) that an implementer cannot settle alone, because each is a contract between the Rust Checker and the independently written Python reference checker, or a hashed record format that a conformance case will freeze. Everything else in that report is implementer's discretion, recorded as it is decided.

Answer compactly, for example `1a 2a 3a 4a 5a 6a 7b 8a 9b 10a 11a 12a`, with a note where you depart. I turn the answers into one patch against the repo copy.

| # | Decision | Recommended |
| --- | --- | --- |
| 1 | Where `IndexTx` lives | a, new `tm-index` crate |
| 2 | Steps schema: start table and `join.on` shape | a, explicit `from`, list of objects |
| 3 | Text form of computed decimals | a, the General Decimal Arithmetic rules |
| 4 | Rendering of computed doubles | a, shortest round-trip, plain notation |
| 5 | `and`, `or`, `not` with `missing` | a, SQL three-valued |
| 6 | How `-5` tokenizes | a, sign is always an operator |
| 7 | `{expr}` in descriptions | b, re-serialized from the parse tree |
| 8 | Tolerance magnitude and the hash | a, round tolerated results to 15 significant digits |
| 9 | `field` path syntax in errors | b, JSON Pointer |
| 10 | `snapshot` and `manifest` schemas | a, as drafted below |
| 11 | List-valued settings | a, a `list` typed value |
| 12 | `claim.revised` citations | a, every revision carries them |

---

## 1. Where `IndexTx` lives

**Question.** Rule 6 has every library expose `fn fold(&mut IndexTx, &Event)`, and 11.17 defines `IndexTx` as a wrapper over a `rusqlite::Transaction`. The only file that could define it today is `tm-core/src/index/mod.rs`, built in P7, but `tm-files` (P3) and `tm-claims` (P6) must name it. As written the Core depends on the libraries and the libraries depend on the Core.

**Why now.** P3's checkpoint is `cargo test -p tm-files`, which cannot compile.

**Options.**
- **a.** A new crate `tm-index` holding `IndexTx` and the `rusqlite` dependency, listed in 4.1 beside `tm-duckdb`, built in P2. Libraries and the Core both depend on it.
- **b.** Put `IndexTx` in `tm-formats`, which then carries `rusqlite` and loses the "pure" label in 4.1.
- **c.** Define a trait `IndexSink` in `tm-formats` (execute, query, no `rusqlite`), implement it in `tm-core`, and have folds take `&mut dyn IndexSink`.

**Recommendation: a.** Same shape as the `tm-duckdb` fix, one dependency in one place, no abstraction over a thing with one implementation. **c** is the purer layering and is worth taking if you expect a second index backend; nothing in V1 suggests one.

**Changes.** 4.1 crate table, 4.2 rule 2, item 76 split, a new item 26b, the P2 row, 11.17's `IndexTx` paragraph.

## 2. Steps schema: start table and `join.on` shape

**Question.** `{"steps":1,"tables":{...},"ops":[...]}` never says which table the pipeline starts from. With one table it is obvious; with a join it is not, and canonical JSON sorts keys so "the first entry" is not a rule. Separately, `join.on` is described as "key pairs" with no JSON shape.

**Why now.** Item 17 (`tm-formats/src/steps.rs`, P1) and item 90's conformance cases both need it, and the Python session will otherwise pick differently.

**Options.**
- **a.** Required top-level `"from": "<table name>"`. `on` as `[{"left":"col","right":"col"}, ...]`.
- **b.** Infer the start table (the one no `join.right` names; `E6002` if ambiguous). `on` as `[["left","right"], ...]`.

**Recommendation: a.** The inference rule is one more thing both implementations must get identically right for no gain, and objects survive future fields where pairs do not.

**Changes.** 11.8 schema and the `join` row, item 17, item 90's case list.

## 3. Text form of computed decimals

**Question.** `dec` is specified as "as written; trailing zeros kept", which defines parsing, not results. What does `1.10 * 2` print, `2.20` or `2.2`? What does `10 / 4` print, `2.5` or `2.50`? What about negative zero?

**Why now.** `final_rows_hash` is a hash of these strings. Two implementations that disagree on trailing zeros disagree on every hash.

**Options.**
- **a.** Adopt the General Decimal Arithmetic specification (IEEE 754-2008 decimal) by reference, at 34 significant digits, rounding half-even. That fixes every case by one named rule: multiplication's scale is the sum of the operands' scales (`1.10 * 2` is `2.20`), addition and subtraction take the larger scale, division is exact when it terminates within 34 digits and otherwise rounds (`10 / 4` is `2.5`). Python's `decimal` module implements it directly; Rust's `bigdecimal` is configured to match.
- **b.** Enumerate the cases in 11.18.2 by hand.
- **c.** Normalize every result, stripping trailing zeros (`1.10 * 2` is `2.2`).

**Recommendation: a**, plus one addition the standard leaves open for our purposes: negative zero is written `0`. Citing a standard is the only version of this that stays correct when someone writes an expression neither of us thought of.

**Changes.** 11.6 `dec` entry, 11.18.2 Precision row, item 15.

## 4. Rendering of computed doubles

**Question.** `ln`, `exp`, `mean`, and `median` produce IEEE doubles that must become text. Rust's `{}` prints `0.00001`; Python's `repr` prints `1e-05`. The expression grammar has no exponent form, so a value printed with one cannot be written as a literal.

**Options.**
- **a.** Shortest round-trip digits, always positional notation, never an exponent. Very large and very small magnitudes become long strings.
- **b.** Shortest round-trip with exponent form outside a stated range, and the grammar gains exponent literals.

**Recommendation: a.** A value in a result that cannot be typed into a subsequent step is a trap, and the magnitudes a published table carries do not reach the point where positional notation hurts. Python needs explicit formatting to do this; note it in the spec so the reference author does not reach for `repr`.

**Changes.** 11.6, 11.18.2, item 15, item 87's notes.

## 5. `and`, `or`, `not` with `missing`

**Question.** 11.18.2's Missing row covers arithmetic and comparison only. Is `true or missing` equal to `true` (SQL three-valued) or to `missing` (strict)?

**Options.**
- **a.** SQL three-valued: `true or missing` is `true`, `false and missing` is `false`, `not missing` is `missing`.
- **b.** Strict: any `missing` operand yields `missing`.

**Recommendation: a.** It matches DuckDB, which runs the relational half, and it matches what anyone who has written SQL expects. The `filter` rule already says only `true` keeps a row, so the two differ only in how early the answer is known. Add: a `where` expression that evaluates to a non-boolean is `E6004`.

**Changes.** 11.18.2 Missing row.

## 6. How `-5` tokenizes

**Question.** The grammar has `INT := "-"? DIGIT+` and `unary := "-" unary`, so `a -5` parses as either a subtraction or two tokens, and the two readings render differently in descriptions.

**Options.**
- **a.** Remove the sign from the literal rules. `-` is always an operator, unary or binary.
- **b.** Keep signed literals and require whitespace to disambiguate.

**Recommendation: a.** One rule, no lookahead, no whitespace sensitivity in a language where whitespace is otherwise ignored.

**Changes.** 11.18.1 `INT` and `DEC` productions, item 17.

## 7. `{expr}` in descriptions

**Question.** 11.18.4 says `{expr}` is "the expression as written after whitespace normalization". Does the description echo the author's text, or re-serialize the parse tree?

**Why now.** Descriptions are compared byte for byte between the two checkers and are part of `expected.json`.

**Options.**
- **a.** Echo the author's text with whitespace runs collapsed.
- **b.** Re-serialize from the parse tree: one space around every binary operator and around `and`, `or`; no space after unary `-`; one space after `not`; one space after a comma inside a call; parentheses emitted only where precedence requires them.

**Recommendation: b.** Under **a** the same claim gets a different description when the author reformats, and the Python side must reproduce the author's spacing exactly rather than a rule. Under **b** the rule is testable from the AST.

**Changes.** 11.18.4 placeholder table, item 17 (the serializer belongs with the parser), item 56.

## 8. Tolerance magnitude, and how a tolerance coexists with the hash

**Question.** `ln`, `exp`, `mean`, and `median` are the tolerated last-digit sites. But `final_rows_hash` hashes decimal strings and is compared exactly, and a one-ulp difference changes the shortest round-trip string entirely, so the tolerance as written cannot apply to any pipeline whose rows the hash covers.

**Evidence from today.** The static musl build is host-independent, and glibc 2.17's `libm` agrees with musl's bit for bit on DuckDB's `ln`, `exp`, and `avg`. Unmeasured: glibc 2.28 and later, which is what CPython will use on the test host. So the tolerance must exist, and its magnitude is not derivable from a measurement we have.

**Options.**
- **a.** Round every `ln`, `exp`, `mean`, and `median` result to 15 significant digits before it enters the final rows. One-ulp differences (around the 16th or 17th digit) disappear, the hash is meaningful for every pipeline, and there is one comparison rule.
- **b.** Keep full precision; the differential suite compares values with a tolerance of one ulp and skips the hash comparison for pipelines containing a tolerated function.

**Recommendation: a**, at 15 significant digits. **b** means two comparison rules and a class of pipeline whose hash is never checked, which is the class most likely to have a bug. The cost of **a** is that a claim can no longer assert the 16th digit of a logarithm, which no claim should.

**Changes.** 11.6, 11.18.2 `log` family row, `final_rows_hash` definition, the Verification Plan's Differential row, `tolerance.json` in item 89.

## 9. `field` path syntax in errors

**Question.** `E6002` and `E6004` name the offending field, and both P1 and P5 checkpoints require Rust and Python to produce the same code and field. No syntax is specified.

**Options.**
- **a.** Dotted with brackets: `ops[1].ties`.
- **b.** JSON Pointer, RFC 6901: `/ops/1/ties`.

**Recommendation: b.** It is a standard with defined escaping, both languages have an implementation, and the conformance files can name it in one line instead of carrying a grammar.

**Changes.** 11.12, items 17 and 20, item 90's invalid-case format (`{"expect":{"code":"E6002","field":"/ops/1/ties"}}`).

## 10. `snapshot` and `manifest` schemas

**Question.** `file.version_added` carries a `snapshot` and, for directories, a `manifest`. Neither has keys. Both are hashed.

**Recommendation: a**, as drafted:

- `snapshot`: `{"size","mtime","mtime_ns","ctime","ctime_ns","ino"}`. `size`, `mtime_ns`, `ctime_ns`, and `ino` are integers; `mtime` and `ctime` are RFC 3339 UTC strings per 11.1, with the nanoseconds in their own integer field. No device number (item 26).
- `manifest`: `{"fingerprint","ignore","entries"}`, where `entries` is a list of `{"path","snapshot","fingerprint"}` sorted by `path` in 11.10 order, `ignore` is the exclude list actually applied, and `fingerprint` is the SHA-256 of the canonical JSON of `entries`.

**Changes.** 11.16, item 26, item 28, item 90.

## 11. List-valued settings

**Question.** `settings.changed` carries `value` and `previous` as "typed values or null", and 11.6 has no list type, but `manifest.ignore` and `harness.env_vars` are lists of strings.

**Options.**
- **a.** Add a `list` typed value to 11.6: `{"t":"list","v":[typed values]}`.
- **b.** Settings values are plain JSON rather than typed values, with the type taken from 11.19.

**Recommendation: a.** One encoding across the whole record is worth more than the saved bytes, and a list type will be wanted again.

**Changes.** 11.6, 11.16 `settings.changed`, item 81, item 83's `settings set` (repeated arguments replace the whole list).

## 12. `claim.revised` citations

**Question.** 11.23 says citation objects live in `claim.added` and `claim.revised` bodies, but the `claim.revised` body has neither `cited` nor `citations`, and `claim_revisions` requires both.

**Options.**
- **a.** Every revision carries the full `cited` and `citations`, so a revision is a complete statement of the claim.
- **b.** A revision omits them and the fold copies from the previous revision row.
- **c.** A revision carries them only when they changed; absence means unchanged.

**Recommendation: a.** The hash of a revision should cover everything the revision asserts, the fold needs no history, and a reader of one event needs no second lookup. The duplication is a few hundred bytes per revision.

**Changes.** 11.16 `claim.revised`, 11.23, item 70, the fold contract in 11.17.
