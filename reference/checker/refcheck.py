#!/usr/bin/env python3
"""Reference checker for one conformance case (checker.md item 92).

    python3 refcheck.py CASE_DIR [--check]

A checker case holds tables/*.csv, specs/*.json, steps.json, and
assertion.json; the output is {verdict, steps, columns, final_rows,
final_rows_hash, key_map_hash, error}. An eval case holds case.json with
`input` (the assertion) and `observed`; the output is {verdict, error}.
With --check the output is compared with the directory's expected.json:
exit 0 on equality, 1 on any difference, naming the differing fields.
"""

import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path[:0] = [HERE, os.path.join(os.path.dirname(HERE), "formats")]

import assertion  # noqa: E402
import ops  # noqa: E402
import tables  # noqa: E402
from canonical_json import FormatError, _parse_int, parse  # noqa: E402
from errors import CheckError  # noqa: E402
from ids import id_of  # noqa: E402
from typed_values import parse_typed  # noqa: E402


def load_json(path):
    with open(path, "rb") as f:
        return parse(f.read())


def error_result(code, field, row_level=True):
    out = {"verdict": "error"}
    if row_level:
        out.update(steps=None, columns=None, final_rows=None, final_rows_hash=None, key_map_hash=None)
    out["error"] = {"code": code, "field": field}
    return out


def check_case(case_dir):
    """Run a checker case; returns the item 92 object."""
    try:
        steps = load_json(os.path.join(case_dir, "steps.json"))
        assertion_doc = load_json(os.path.join(case_dir, "assertion.json"))
        specs = {}
        specs_dir = os.path.join(case_dir, "specs")
        for name in sorted(os.listdir(specs_dir)):
            if name.endswith(".json"):
                specs[name[:-5]] = load_json(os.path.join(specs_dir, name))
    except (FormatError, CheckError) as e:
        return error_result(e.code, e.field)
    try:
        # Item 92 (checker-diffs D5, ruling 2026-11-20): every table in
        # tables/ is parsed before the steps and the assertion are validated,
        # as the product parses a table at table.spec before any claim
        # exists; a cell error is the case's error whatever they hold.
        loaded = {}
        tables_dir = os.path.join(case_dir, "tables")
        for file_name in sorted(os.listdir(tables_dir)):
            if not file_name.endswith(".csv"):
                continue
            name = file_name[:-4]
            if name not in specs:
                raise SystemExit(f"refcheck: no spec for table file {file_name}")
            with open(os.path.join(tables_dir, file_name), "rb") as f:
                data = f.read()
            loaded[name] = tables.in_key_order(tables.read_table(data, specs[name]))

        parsed = ops.validate_shape(steps)
        for name in steps["tables"]:
            if name not in specs:
                raise CheckError("E6002", f"no parse spec for table {name!r}", ops.pointer("tables", name))
        schema = ops.validate_against_specs(steps, parsed, specs)
        parsed_assertion = assertion.validate_shape(assertion_doc)
        assertion.validate_against_schema(parsed_assertion, schema.types, schema.key)

        def load_table(name):
            return loaded[name]

        step_results, final = ops.run(steps, parsed, load_table)
        observed = assertion.observe(parsed_assertion, final)
        verdict = assertion.evaluate(parsed_assertion, observed)
    except (FormatError, CheckError) as e:
        return error_result(e.code, e.field)
    final_rows = [{n: row[n].to_json() for n, _ in final.columns} for row in final.rows]
    key_map = [[v.to_json() for v in final.key_tuple(row)] for row in final.rows]
    return {
        "verdict": verdict,
        "steps": step_results,
        "columns": final.names,
        "final_rows": final_rows,
        "final_rows_hash": id_of(final_rows),
        "key_map_hash": id_of(key_map),
        "error": None,
    }


def check_eval_case(case_dir):
    """Run an assertion evaluation case; returns {verdict, error}."""
    try:
        case = load_json(os.path.join(case_dir, "case.json"))
        parsed = assertion.validate_shape(case["input"])
        observed = {}
        for k, v in case["observed"].items():
            if k in ("cell", "left", "right") and v is not None:
                observed[k] = parse_typed(v, f"/observed/{k}")
            else:
                observed[k] = v
        verdict = assertion.evaluate(parsed, observed)
    except (FormatError, CheckError) as e:
        return error_result(e.code, e.field, row_level=False)
    return {"verdict": verdict, "error": None}


def diff(expected, actual, path=""):
    """Paths whose values differ between two JSON values."""
    if isinstance(expected, dict) and isinstance(actual, dict):
        out = []
        for k in sorted(set(expected) | set(actual)):
            if k not in expected or k not in actual:
                out.append(f"{path}/{k}: {'absent' if k not in actual else 'unexpected'}")
            else:
                out += diff(expected[k], actual[k], f"{path}/{k}")
        return out
    if isinstance(expected, list) and isinstance(actual, list):
        if len(expected) != len(actual):
            return [f"{path}: {len(expected)} items expected, {len(actual)} produced"]
        out = []
        for i, (e, a) in enumerate(zip(expected, actual)):
            out += diff(e, a, f"{path}/{i}")
        return out
    if expected != actual or type(expected) is not type(actual):
        return [f"{path}: expected {json.dumps(expected, ensure_ascii=False)}, got {json.dumps(actual, ensure_ascii=False)}"]
    return []


def main(argv):
    args = [a for a in argv[1:] if not a.startswith("--")]
    check = "--check" in argv
    if len(args) != 1:
        print(__doc__, file=sys.stderr)
        return 2
    case_dir = args[0]
    if os.path.exists(os.path.join(case_dir, "case.json")):
        result = check_eval_case(case_dir)
    else:
        result = check_case(case_dir)
    if not check:
        print(json.dumps(result, indent=2, ensure_ascii=False))
        return 0
    with open(os.path.join(case_dir, "expected.json"), "rb") as f:
        # A bare integer of more than 4,300 digits would raise ValueError at
        # CPython's int-string limit under json's default parse_int;
        # canonical_json's _parse_int checks the digit count on the text
        # first (P7 owner ruling 2, 2026-11-27).
        expected = json.loads(f.read().decode("utf-8"), parse_int=_parse_int)
    differences = diff(expected, result)
    if differences:
        print(f"{case_dir}: {len(differences)} difference(s)")
        for d in differences:
            print("  " + d)
        return 1
    print(f"{case_dir}: ok")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
