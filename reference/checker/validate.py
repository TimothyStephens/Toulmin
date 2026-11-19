#!/usr/bin/env python3
"""Validate steps and assertion documents (checker.md 11.8 and 11.9).

    python3 validate.py [--check] FILE...

Each FILE is a conformance schema case: {"description", "input", ...,
"expect"}. A steps case carries "specs" (name -> parse spec) and its input
is checked against them; an assertion case may carry "table"
({"columns": [[name, type], ...], "key": [...]}) and its input is then
checked against that schema. Per file the script prints `ok` or the error
code and JSON Pointer field. With --check it also compares the result with
the file's "expect" and exits 1 when any file differs.
"""

import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path[:0] = [HERE, os.path.join(os.path.dirname(HERE), "formats")]

import assertion  # noqa: E402
import ops  # noqa: E402
from canonical_json import FormatError, parse  # noqa: E402
from errors import CheckError  # noqa: E402


def validate_steps(doc, specs):
    parsed = ops.validate_shape(doc)
    if specs is not None:
        ops.validate_against_specs(doc, parsed, specs)


def validate_assertion(doc, table):
    parsed = assertion.validate_shape(doc)
    if table is not None:
        columns = {name: t for name, t in table["columns"]}
        assertion.validate_against_schema(parsed, columns, table["key"])


def validate_file(path):
    """Returns (code, field) or None for a valid input."""
    with open(path, "rb") as f:
        case = parse(f.read())
    doc = case["input"]
    try:
        if isinstance(doc, dict) and "assert" in doc and "steps" not in doc:
            validate_assertion(doc, case.get("table"))
        else:
            validate_steps(doc, case.get("specs"))
    except (CheckError, FormatError) as e:
        return e.code, e.field
    return None


def main(argv):
    check = "--check" in argv
    files = [a for a in argv[1:] if not a.startswith("--")]
    if not files:
        print(__doc__, file=sys.stderr)
        return 2
    failures = 0
    for path in files:
        try:
            result = validate_file(path)
        except FormatError as e:
            print(f"{path}: {e.code} {e.field or ''} (case file itself)".rstrip())
            failures += 1
            continue
        text = "ok" if result is None else f"{result[0]} {result[1]}"
        if not check:
            print(f"{path}: {text}")
            continue
        with open(path, "rb") as f:
            expect = parse(f.read())["expect"]
        if expect.get("valid"):
            want = None
        else:
            want = (expect["code"], expect["field"])
        if want == result:
            print(f"{path}: {text}")
        else:
            failures += 1
            want_text = "ok" if want is None else f"{want[0]} {want[1]}"
            print(f"{path}: {text}  MISMATCH, expected {want_text}")
    if check:
        print(f"{len(files) - failures} of {len(files)} match")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
