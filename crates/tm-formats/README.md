# tm-formats

Toulmin's shared formats: the choices of design section 11 that every
other crate takes from here. Pure: no state and no I/O, apart from the
optional `TM_ERROR_LOG` append when an error is constructed.

| Module | Design | What |
| --- | --- | --- |
| `canonical` | 11.1 | Strict I-JSON reader and RFC 8785 serializer; `E1001`, `E1003` |
| `id` | 11.2, 11.16 | `sha256:` IDs, genesis strings, `F<n>`, `F<n>@v<m>`, `C<n>`, `R<n>` handles |
| `envelope` | 11.2, 11.11 | Envelope validation, the line form with `id` appended; `E1002` |
| `events` | 11.16 | The closed gate A event type set and each body's schema |
| `typed_value` | 11.6 | Typed values, their lexical rules (`E1004`), IEEE-double conversions, Arrow mapping (item 47) |
| `parse_spec` | 11.7 | Parse spec schema and fingerprint; `E3103`, `E3104`, `E3105` |
| `steps`, `expr` | 11.8, 11.18.1, 11.18.2, 11.18.4 | Ops, expression parser and `{expr}` serializer, column and type checks; `E6002`, `E6004`, `E6005`, `E6006` |
| `assertion` | 11.9, 11.18.3 | Assertion schema and comparison rules |
| `citation` | 11.23 | Citation objects and the CLI spelling; `E6007` |
| `errors` | 11.12 | The catalog (`src/catalog.rs`) and error values |
| `version` | 11.11 | Version constants and the reader policy |
| `sort_order` | 11.10 | Binary order of NFC bytes |

`build.rs` generates the `Code` enum and `docs/errors.md` from
`src/catalog.rs`; set `TM_ERRORS_OUT` to also write the Markdown where
`scripts/ci/spec-drift.sh` can compare it with the committed file.

## Tests

- `cargo test -p tm-formats` runs the unit tests, `tests/rfc8785.rs`, and
  `tests/conformance.rs`.
- `python3 reference/formats/test_formats.py` is the second canonical-JSON
  and ID implementation.
- With `TM_FORMATS_IDS_OUT=FILE` set, each writes one line per conformance
  case, `<path> <file id> <subject id>`; the two files must be identical
  (the P1 checkpoint).

## Conformance case format (`conformance/formats/`)

Every case is a JSON object with a `description` and an `expect`. Errors
are expected as `{"code","field"}` with `field` a JSON Pointer into the
case's input (`""` for an expression string; `null` where no pointer
applies).

| Directory | Input | `expect` |
| --- | --- | --- |
| `canonical/` | `input_text`: JSON text | `{"canonical","id"}` or `{"code"}` |
| `ids/` | `input`: any JSON, or `genesis`: `{"chain","project_id"}` | `{"id"}` |
| `envelope/` | `line`: one envelope line | `{"id","type","event"}` with `event` `known` or `opaque`, or `{"code","field"}`; `<type>.valid.json` and `<type>.invalid.json` exist for every gate A type |
| `typed_values/` | `case: "typed_value"` with `input` a typed value; or `case: "from_f64"` with `input` `{"f64": "0x<IEEE bits>", "rule": "round15" \| "shortest"}` | `{"valid": true}`, with `normalized` (the value as read) when it differs from `input`, or `{"code","field"}`; `{"dec"}` for `from_f64` |
| `steps/valid/`, `steps/invalid/` | `input`: steps; `specs`: table name to parse spec | `{"valid": true}` or `{"code","field"}` |
| `expr/` | `input`: expression string; `columns`: column name to spec type | `{"render","type"}` (`type` `null` for an untyped `missing`) or `{"code","field"}` |
| `assertions/schema/` | `input`: an assertion | `{"valid": true}` or `{"code","field"}` |
| `conformance/checker/assertions/eval/` (item 92, not read by this crate's tests) | `input`: an assertion; `observed` | `{"outcome"}` or `{"code","field"}` |

The `eval/` cases are the 11.18.3 rules, a Checker fact frozen at P4 by
`tm-checker`'s conformance test; they are generated beside the schema
cases and share this format. `observed` is what the Checker located: `{"cell": <typed value or null>}`
for `value`, `{"position": <int or null>}` for `rank`, `{"count": <int>}`
for `count`, `{"left","right"}` (typed values or `null`) for
`compare_rows`, `{"present": <bool>}` for `exists`; `null` means the row is
absent.

The subject ID of a case is the ID of `input` (`canonical/`: of the parsed
`input_text`; `envelope/`: of the line without `id`; `ids/`: the computed
ID), or the refusal code when there is none.
