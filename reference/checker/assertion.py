"""Assertions: schema (checker.md 11.9) and evaluation rules (11.18.3).

`validate_shape` checks an assertion without a table schema and returns a
parsed form; `validate_against_schema` checks it against the final table's
columns and key; `observe` locates what the assertion names in the final
table; `evaluate` gives the verdict from an observation.
"""

from decimal import Decimal

from errors import CheckError
from typed_values import (
    DEC_RE,
    EXACT,
    NUMBER_TYPES,
    Value,
    as_decimal,
    compare_present,
    comparison_class,
    equal,
    parse_typed,
    types_compare,
)

ASSERT_VERSION = 1
OPS = ("eq", "ne", "lt", "le", "gt", "ge", "between")
FIELDS = {
    "value": ["row", "column", "op", "expected"],
    "rank": ["row", "expected"],
    "count": ["op", "expected"],
    "compare_rows": ["left", "right", "op"],
    "exists": ["row"],
}
TAKES_TOL = ("value", "rank", "compare_rows")
INT_EXPECTED = ("rank", "count")


def _absent(field):
    return CheckError("E6002", "required member is absent", field)


def _wrong(field, msg):
    return CheckError("E6004", msg, field)


def _not_allowed(field, msg):
    return CheckError("E6002", msg, field)


# --- shape ------------------------------------------------------------------


def validate_shape(a):
    """Check an assertion's shape (11.9). Returns a dict: kind, op, row (list
    of Value), column, left/right ({row, column}), expected (Value or [lo,
    hi]), tol ((abs, rel) Decimals or None)."""
    if not isinstance(a, dict):
        raise _wrong("", "assertion is an object")
    if "assert" not in a:
        raise _absent("/assert")
    if not isinstance(a["assert"], int) or isinstance(a["assert"], bool):
        raise _wrong("/assert", "assert is an integer")
    if a["assert"] != ASSERT_VERSION:
        raise _not_allowed("/assert", f"unknown assertion version {a['assert']}")
    if "kind" not in a:
        raise _absent("/kind")
    kind = a["kind"]
    if not isinstance(kind, str) or kind not in FIELDS:
        raise _not_allowed("/kind", f"unknown kind {kind!r}")
    fields = FIELDS[kind]
    for key in fields:
        if key not in a:
            raise _absent("/" + key)
    allowed = set(fields) | {"assert", "kind"}
    if kind in TAKES_TOL:
        allowed.add("tol")
    for key in a:
        if key not in allowed:
            raise _not_allowed("/" + key, f"{key!r} is not a member of a {kind} assertion")
    out = {"kind": kind, "op": None, "tol": None}
    if "row" in fields:
        out["row"] = _row(a["row"], "/row")
    if "column" in fields:
        out["column"] = _column(a["column"], "/column")
    if kind == "compare_rows":
        for side in ("left", "right"):
            out[side] = _side(a[side], "/" + side)
    if "op" in fields:
        op = a["op"]
        if not isinstance(op, str) or op not in OPS:
            raise _not_allowed("/op", f"unknown op {op!r}")
        if kind == "compare_rows" and op == "between":
            raise _not_allowed("/op", "compare_rows does not take between")
        out["op"] = op
    elif kind == "rank":
        out["op"] = "eq"
    if "expected" in fields:
        out["expected"] = _expected(a["expected"], out["op"], kind)
    if "tol" in a:
        out["tol"] = _tol(a["tol"], "/tol")
        if kind == "value":
            exp = out["expected"]
            t = exp[0].t if isinstance(exp, list) else exp.t
            if t not in NUMBER_TYPES:
                raise _wrong("/tol", f"tol does not apply to {t}")
    return out


def _row(row, field):
    if not isinstance(row, list):
        raise _wrong(field, "row is a list of typed values")
    if not row:
        raise _not_allowed(field, "row names at least one key value")
    values = []
    for i, item in enumerate(row):
        v = parse_typed(item, f"{field}/{i}")
        if v.t in ("nan", "inf", "list"):
            raise _wrong(f"{field}/{i}", "a row value is a value or missing")
        values.append(v)
    return values


def _column(column, field):
    if not isinstance(column, str):
        raise _wrong(field, "column is a string")
    return column


def _side(side, field):
    if not isinstance(side, dict):
        raise _wrong(field, "an object with row and column is required")
    for key in ("row", "column"):
        if key not in side:
            raise _absent(f"{field}/{key}")
    for key in side:
        if key not in ("row", "column"):
            raise _not_allowed(f"{field}/{key}", f"{key!r} is not a member here")
    return {"row": _row(side["row"], field + "/row"), "column": _column(side["column"], field + "/column")}


def _one_expected(obj, field, kind):
    v = parse_typed(obj, field)
    if v.absent or v.t == "list":
        raise _wrong(field, "expected is never missing, nan, inf, or list")
    if kind in INT_EXPECTED and v.t != "int":
        raise _wrong(field, f"{kind} expects an int")
    return v


def _expected(obj, op, kind):
    if op == "between":
        if not isinstance(obj, list) or len(obj) != 2:
            raise _wrong("/expected", "between expects [lo, hi]")
        lo = _one_expected(obj[0], "/expected/0", kind)
        hi = _one_expected(obj[1], "/expected/1", kind)
        if not types_compare(lo.t, hi.t):
            raise _wrong("/expected/1", "lo and hi are of one comparison class")
        if lo.t in ("str", "bool"):
            raise _wrong("/op", f"between does not apply to {lo.t}")
        return [lo, hi]
    v = _one_expected(obj, "/expected", kind)
    if v.t in ("str", "bool") and op not in ("eq", "ne"):
        raise _wrong("/op", f"{op} does not apply to {v.t}")
    return v


def _tol(tol, field):
    if not isinstance(tol, dict):
        raise _wrong(field, "tol is an object")
    for key in ("abs", "rel"):
        if key not in tol:
            raise _absent(f"{field}/{key}")
    for key in tol:
        if key not in ("abs", "rel"):
            raise _not_allowed(f"{field}/{key}", f"{key!r} is not a member of tol")
    parts = []
    for key in ("abs", "rel"):
        v = tol[key]
        if not isinstance(v, str) or not DEC_RE.fullmatch(v):
            raise _wrong(f"{field}/{key}", f"{key} is a decimal in the dec v form")
        if v.startswith("-"):
            raise _wrong(f"{field}/{key}", f"{key} is non-negative")
        parts.append(Decimal(v))
    return tuple(parts)


# --- against the final table's schema --------------------------------------


def validate_against_schema(parsed, columns, key):
    """`columns` is a dict name -> type, `key` the key column names."""
    kind = parsed["kind"]
    if kind == "count":
        return
    if kind == "compare_rows":
        for side in ("left", "right"):
            _check_row(parsed[side]["row"], columns, key, f"/{side}/row")
            col = parsed[side]["column"]
            if col not in columns:
                raise CheckError("E6005", f"unknown column {col!r}", f"/{side}/column")
        lt, rt = columns[parsed["left"]["column"]], columns[parsed["right"]["column"]]
        if not types_compare(lt, rt):
            raise _wrong("/right/column", f"{lt} and {rt} do not compare")
        _check_op_for_type(parsed["op"], lt, parsed["tol"])
        return
    _check_row(parsed["row"], columns, key, "/row")
    if kind == "exists" or kind == "rank":
        return
    col = parsed["column"]
    if col not in columns:
        raise CheckError("E6005", f"unknown column {col!r}", "/column")
    ct = columns[col]
    exp = parsed["expected"]
    for e in exp if isinstance(exp, list) else [exp]:
        if not types_compare(e.t, ct):
            raise _wrong("/expected", f"{e.t} does not compare with {ct}")


def _check_row(row, columns, key, field):
    if len(row) != len(key):
        raise _wrong(field, f"row has {len(row)} values for {len(key)} key columns")
    for i, (v, k) in enumerate(zip(row, key)):
        if v.t == "missing":
            continue
        if not types_compare(v.t, columns[k]):
            raise _wrong(f"{field}/{i}", f"{v.t} is not the key column's type {columns[k]}")


def _check_op_for_type(op, t, tol):
    if t in ("str", "bool") and op not in ("eq", "ne"):
        raise _wrong("/op", f"{op} does not apply to {t}")
    if tol is not None and t not in NUMBER_TYPES:
        raise _wrong("/tol", f"tol does not apply to {t}")


# --- locating rows (11.18.3) ------------------------------------------------


def matches(row_values, key_values):
    for want, have in zip(row_values, key_values):
        if want.t == "missing":
            if not have.absent:
                return False
        elif not equal(want, have):
            return False
    return True


def locate(table, row_values):
    """(row, 1-based position) or (None, None)."""
    for pos, row in enumerate(table.rows, start=1):
        if matches(row_values, table.key_tuple(row)):
            return row, pos
    return None, None


def observe(parsed, table):
    """What the Checker located, in the eval case `observed` shape."""
    kind = parsed["kind"]
    if kind == "count":
        return {"count": len(table.rows)}
    if kind == "compare_rows":
        out = {}
        for side in ("left", "right"):
            row, _ = locate(table, parsed[side]["row"])
            out[side] = None if row is None else row[parsed[side]["column"]]
        return out
    row, pos = locate(table, parsed["row"])
    if kind == "exists":
        return {"present": row is not None}
    if kind == "rank":
        return {"position": pos}
    return {"cell": None if row is None else row[parsed["column"]]}


# --- evaluation -------------------------------------------------------------


def evaluate(parsed, observed):
    """The verdict, `holds` or `fails`; raises CheckError E6004 for a type
    the comparison does not allow (11.9, 11.18.3)."""
    kind = parsed["kind"]
    tol = parsed["tol"]
    if kind == "exists":
        return verdict(bool(observed["present"]))
    if kind == "count":
        a = Value("int", observed["count"])
        return verdict(compare(parsed["op"], a, parsed["expected"], None))
    if kind == "rank":
        pos = observed["position"]
        if pos is None:
            return "fails"
        return verdict(compare("eq", Value("int", pos), parsed["expected"], tol))
    if kind == "compare_rows":
        a, b = observed["left"], observed["right"]
        if a is None or b is None or a.absent or b.absent:
            return "fails"
        if not types_compare(a.t, b.t):
            raise _wrong("/right", f"{a.t} does not compare with {b.t}")
        if tol is not None and not a.is_number:
            raise _wrong("/tol", f"tol does not apply to {a.t}")
        return verdict(compare(parsed["op"], a, b, tol))
    cell = observed["cell"]
    if cell is None or cell.absent:
        return "fails"
    exp = parsed["expected"]
    e0 = exp[0] if isinstance(exp, list) else exp
    if not types_compare(cell.t, e0.t):
        raise _wrong("/expected", f"{cell.t} does not compare with {e0.t}")
    if tol is not None and not cell.is_number:
        raise _wrong("/tol", f"tol does not apply to {cell.t}")
    return verdict(compare(parsed["op"], cell, exp, tol))


def verdict(ok):
    return "holds" if ok else "fails"


def margin(tol, b):
    """m = abs + rel * |b|, computed exactly."""
    if tol is None:
        return Decimal(0)
    abs_, rel = tol
    return EXACT.add(abs_, EXACT.multiply(rel, b.copy_abs()))


def compare(op, a, expected, tol):
    """`a` against `expected` (a Value, or [lo, hi] for between) under `op`."""
    if a.is_number:
        da = as_decimal(a)
        if op == "between":
            lo, hi = as_decimal(expected[0]), as_decimal(expected[1])
            low = EXACT.subtract(lo, margin(tol, lo))
            high = EXACT.add(hi, margin(tol, hi))
            return low <= da <= high
        db = as_decimal(expected)
        m = margin(tol, db)
        diff = EXACT.subtract(da, db)
        if op == "eq":
            return diff.copy_abs() <= m
        if op == "ne":
            return diff.copy_abs() > m
        if op == "lt":
            return da < EXACT.add(db, m)
        if op == "le":
            return da <= EXACT.add(db, m)
        if op == "gt":
            return da > EXACT.subtract(db, m)
        return da >= EXACT.subtract(db, m)
    if a.t in ("str", "bool"):
        if op == "eq":
            return equal(a, expected)
        if op == "ne":
            return not equal(a, expected)
        raise _wrong("/op", f"{op} does not apply to {a.t}")
    # date, datetime: chronological
    if op == "between":
        return compare_present(expected[0], a) <= 0 and compare_present(a, expected[1]) <= 0
    c = compare_present(a, expected)
    return {"eq": c == 0, "ne": c != 0, "lt": c < 0, "le": c <= 0, "gt": c > 0, "ge": c >= 0}[op]
