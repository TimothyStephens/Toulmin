"""Query steps (checker.md 11.8) and their semantics (11.18.2).

`validate_shape` checks a steps document without parse specs, op by op;
`validate_against_specs` checks it against the cited specs, op by op, and
returns the final table's schema; `run` executes it.
"""

import decimal
import functools
import math
import re

import describe
import exprs
from canonical_json import canonical
from errors import CheckError
from ids import id_of
from tables import Table, check_spec, spec_columns
from typed_values import (
    CONTEXT,
    MISSING,
    NUMBER_TYPES,
    Value,
    as_decimal,
    compare_present,
    dec_from_double,
    equal,
    identity,
    types_compare,
)

STEPS_VERSION = 1
OP_VERSIONS = {"filter": 1, "select": 1, "sort": 1, "take": 1, "rank": 1, "derive": 1, "group": 1, "join": 1}
REQUIRED = {
    "filter": ["where"],
    "select": ["columns"],
    "sort": ["by", "missing"],
    "take": ["n"],
    "rank": ["by", "dir", "ties", "missing", "scope"],
    "derive": ["as", "expr"],
    "group": ["by", "aggregates"],
    "join": ["right", "on", "kind"],
}
AGGREGATES = ("count", "sum", "mean", "min", "max", "median")
HANDLE_RE = re.compile(r"F[1-9][0-9]*@v[1-9][0-9]*")
GROUP_EXTRA = ("member_count", "member_keys_hash")


def pointer(*parts):
    """A JSON Pointer (RFC 6901) from member names and indexes."""
    out = []
    for p in parts:
        p = str(p).replace("~", "~0").replace("/", "~1")
        out.append("/" + p)
    return "".join(out)


def absent(field, what="member"):
    return CheckError("E6002", f"required {what} is absent", field)


def wrong_type(field, what):
    return CheckError("E6004", f"{what}", field)


def not_a_choice(field, value, choices):
    return CheckError("E6002", f"{value!r} is not one of {', '.join(choices)}", field)


def _is_int(x):
    return isinstance(x, int) and not isinstance(x, bool)


def _name_list(value, field, allow_empty=False):
    if not isinstance(value, list):
        raise wrong_type(field, "a list of column names is required")
    if not value and not allow_empty:
        raise absent(field, "list is empty; a non-empty list")
    for j, name in enumerate(value):
        if not isinstance(name, str):
            raise wrong_type(f"{field}/{j}", "a column name is a string")
        if name in value[:j]:
            raise CheckError("E6002", f"{name!r} appears twice", f"{field}/{j}")
    return value


def _choice(obj, key, field, choices):
    v = obj[key]
    if not isinstance(v, str):
        raise wrong_type(f"{field}/{key}", f"{key} is a string")
    if v not in choices:
        raise not_a_choice(f"{field}/{key}", v, choices)
    return v


def _string(obj, key, field):
    v = obj[key]
    if not isinstance(v, str):
        raise wrong_type(f"{field}/{key}", f"{key} is a string")
    return v


def _name(obj, key, field):
    v = _string(obj, key, field)
    if not v:
        raise wrong_type(f"{field}/{key}", f"{key} is a non-empty string")
    return v


def _object(value, field, keys):
    if not isinstance(value, dict):
        raise wrong_type(field, "an object is required")
    for k in keys:
        if k not in value:
            raise absent(f"{field}/{k}")
    for k in value:
        if k not in keys:
            raise CheckError("E6002", f"{k!r} is not a member here", f"{field}/{k}")


def _expression(text, field):
    try:
        return exprs.parse(text)
    except CheckError as e:
        raise e.at(field)


# --- shape (no parse spec needed) -------------------------------------------


def validate_shape(steps):
    """Check a steps document's shape (11.8). Returns the parsed expressions
    by op index: {i: node}."""
    if not isinstance(steps, dict):
        raise wrong_type("", "steps is an object")
    if "steps" not in steps:
        raise absent("/steps")
    if not _is_int(steps["steps"]):
        raise wrong_type("/steps", "steps is an integer")
    if steps["steps"] != STEPS_VERSION:
        raise not_a_choice("/steps", steps["steps"], [str(STEPS_VERSION)])
    if "tables" not in steps:
        raise absent("/tables")
    tables = steps["tables"]
    if not isinstance(tables, dict):
        raise wrong_type("/tables", "tables is an object")
    if not tables:
        raise absent("/tables", "object is empty; a non-empty object")
    for name, handle in tables.items():
        if not isinstance(handle, str) or not HANDLE_RE.fullmatch(handle):
            raise wrong_type(pointer("tables", name), "a version handle is required")
    if "from" not in steps:
        raise absent("/from")
    if not isinstance(steps["from"], str):
        raise wrong_type("/from", "from is a string")
    if steps["from"] not in tables:
        raise CheckError("E6002", f"from names no entry of tables", "/from")
    if "ops" not in steps:
        raise absent("/ops")
    if not isinstance(steps["ops"], list):
        raise wrong_type("/ops", "ops is a list")
    for k in steps:
        if k not in ("steps", "tables", "from", "ops"):
            raise CheckError("E6002", f"{k!r} is not a member of steps", pointer(k))
    parsed = {}
    ordered = False
    for i, op in enumerate(steps["ops"]):
        f = f"/ops/{i}"
        if not isinstance(op, dict):
            raise wrong_type(f, "an op is an object")
        if "op" not in op:
            raise absent(f + "/op")
        name = op["op"]
        if not isinstance(name, str):
            raise wrong_type(f + "/op", "op is a string")
        if name not in OP_VERSIONS:
            raise not_a_choice(f + "/op", name, list(OP_VERSIONS))
        if "v" not in op:
            raise absent(f + "/v")
        if not _is_int(op["v"]) or op["v"] < 1:
            raise wrong_type(f + "/v", "v is a positive integer")
        if op["v"] != OP_VERSIONS[name]:
            raise not_a_choice(f + "/v", op["v"], [str(OP_VERSIONS[name])])
        for key in REQUIRED[name]:
            if key not in op:
                raise absent(f"{f}/{key}")
        allowed = set(REQUIRED[name]) | {"op", "v"}
        if name == "rank":
            scope = _choice(op, "scope", f, ("all", "group"))
            if scope == "group":
                if "group_by" not in op:
                    raise absent(f + "/group_by")
                allowed.add("group_by")
            elif "group_by" in op:
                raise CheckError("E6002", "group_by is defined only when scope is group", f + "/group_by")
        for key in op:
            if key not in allowed:
                raise CheckError("E6002", f"{key!r} is not a member of {name}", f"{f}/{key}")
        if name == "filter":
            parsed[i] = _expression(_string(op, "where", f), f + "/where")
        elif name == "select":
            _name_list(op["columns"], f + "/columns")
        elif name == "sort":
            by = op["by"]
            if not isinstance(by, list):
                raise wrong_type(f + "/by", "by is a list")
            if not by:
                raise absent(f + "/by", "list is empty; a non-empty list")
            for j, entry in enumerate(by):
                ef = f"{f}/by/{j}"
                _object(entry, ef, ("column", "dir"))
                column = _string(entry, "column", ef)
                if column in [e["column"] for e in by[:j]]:
                    raise CheckError("E6002", f"{column!r} appears twice", ef + "/column")
                _choice(entry, "dir", ef, ("asc", "desc"))
            _choice(op, "missing", f, ("first", "last"))
        elif name == "take":
            n = op["n"]
            if not _is_int(n):
                raise wrong_type(f + "/n", "n is a JSON integer")
            if n < 0:
                raise wrong_type(f + "/n", "n is at least 0")
            if not ordered:
                raise CheckError("E6002", "take needs an earlier sort or rank", f)
        elif name == "rank":
            _string(op, "by", f)
            _choice(op, "dir", f, ("asc", "desc"))
            _choice(op, "ties", f, ("min", "max", "dense", "ordinal"))
            _choice(op, "missing", f, ("first", "last", "exclude"))
            if op["scope"] == "group":
                _name_list(op["group_by"], f + "/group_by")
        elif name == "derive":
            _name(op, "as", f)
            parsed[i] = _expression(_string(op, "expr", f), f + "/expr")
        elif name == "group":
            _name_list(op["by"], f + "/by")
            aggs = op["aggregates"]
            if not isinstance(aggs, list):
                raise wrong_type(f + "/aggregates", "aggregates is a list")
            if not aggs:
                raise absent(f + "/aggregates", "list is empty; a non-empty list")
            for j, a in enumerate(aggs):
                af = f"{f}/aggregates/{j}"
                _object(a, af, ("as", "fn", "column"))
                _name(a, "as", af)
                _choice(a, "fn", af, AGGREGATES)
                _string(a, "column", af)
        elif name == "join":
            right = _string(op, "right", f)
            if right not in tables:
                raise CheckError("E6002", f"right names no entry of tables", f + "/right")
            on = op["on"]
            if not isinstance(on, list):
                raise wrong_type(f + "/on", "on is a list")
            if not on:
                raise absent(f + "/on", "list is empty; a non-empty list")
            for j, pair in enumerate(on):
                pf = f"{f}/on/{j}"
                _object(pair, pf, ("left", "right"))
                _string(pair, "left", pf)
                _string(pair, "right", pf)
            _choice(op, "kind", f, ("inner", "left"))
        if name in ("sort", "rank"):
            ordered = True
    return parsed


# --- against the cited parse specs -----------------------------------------


class Schema:
    """Column names with types, in order, and the key."""

    def __init__(self, columns, key):
        self.columns = list(columns)
        self.key = list(key)

    @property
    def types(self):
        return dict(self.columns)

    @property
    def names(self):
        return [n for n, _ in self.columns]


def validate_against_specs(steps, parsed, specs):
    """Check the steps against the cited specs (11.8). `specs` maps a table
    name to its parse spec. Returns the final Schema."""
    for name in steps["tables"]:
        if name in specs:
            check_spec(specs[name])
    schema = Schema(spec_columns(specs[steps["from"]]), specs[steps["from"]]["key"])
    for i, op in enumerate(steps["ops"]):
        schema = _check_op(i, op, parsed.get(i), schema, specs)
    return schema


def _check_columns(names, schema, field):
    for j, n in enumerate(names):
        if n not in schema.types:
            raise CheckError("E6005", f"unknown column {n!r}", f"{field}/{j}")


def _check_op(i, op, expr, schema, specs):
    f = f"/ops/{i}"
    name = op["op"]
    types = schema.types
    if name == "filter":
        try:
            t = exprs.infer(expr, types)
        except CheckError as e:
            raise e.at(f + "/where")
        if t is not None and t != "bool":
            raise CheckError("E6004", f"where gives {t}, not a boolean", f + "/where")
        return schema
    if name == "select":
        _check_columns(op["columns"], schema, f + "/columns")
        keep = set(op["columns"]) | set(schema.key)
        return Schema([c for c in schema.columns if c[0] in keep], schema.key)
    if name == "sort":
        for j, entry in enumerate(op["by"]):
            if entry["column"] not in types:
                raise CheckError("E6005", f"unknown column {entry['column']!r}", f"{f}/by/{j}/column")
        return schema
    if name == "take":
        return schema
    if name == "rank":
        if op["by"] not in types:
            raise CheckError("E6005", f"unknown column {op['by']!r}", f + "/by")
        if op["scope"] == "group":
            _check_columns(op["group_by"], schema, f + "/group_by")
        if "rank" in types:
            raise CheckError("E6002", "column rank is already present", f)
        return Schema(schema.columns + [("rank", "int")], schema.key)
    if name == "derive":
        if op["as"] in types:
            raise CheckError("E6002", f"column {op['as']!r} is already present", f + "/as")
        try:
            t = exprs.infer(expr, types)
        except CheckError as e:
            raise e.at(f + "/expr")
        if t is None:
            raise CheckError("E6004", "derive result has no type", f + "/expr")
        return Schema(schema.columns + [(op["as"], t)], schema.key)
    if name == "group":
        _check_columns(op["by"], schema, f + "/by")
        columns = [(b, types[b]) for b in op["by"]]
        names = set(op["by"])
        for j, a in enumerate(op["aggregates"]):
            af = f"{f}/aggregates/{j}"
            col = a["column"]
            if col not in types:
                raise CheckError("E6005", f"unknown column {col!r}", af + "/column")
            t = aggregate_type(a["fn"], types[col], af + "/column")
            if a["as"] in names or a["as"] in GROUP_EXTRA:
                raise CheckError("E6002", f"column {a['as']!r} is already present", af + "/as")
            names.add(a["as"])
            columns.append((a["as"], t))
        columns += [("member_count", "int"), ("member_keys_hash", "str")]
        return Schema(columns, op["by"])
    if name == "join":
        right = op["right"]
        rspec = specs[right]
        rtypes = dict(spec_columns(rspec))
        covered = set()
        for j, pair in enumerate(op["on"]):
            pf = f"{f}/on/{j}"
            if pair["left"] not in types:
                raise CheckError("E6005", f"unknown column {pair['left']!r}", pf + "/left")
            if pair["right"] not in rtypes:
                raise CheckError("E6005", f"unknown column {pair['right']!r}", pf + "/right")
            if not types_compare(types[pair["left"]], rtypes[pair["right"]]):
                raise CheckError("E6004", "on pair types do not compare", pf)
            covered.add(pair["right"])
        if not set(rspec["key"]) <= covered:
            raise CheckError("E6002", "on pairs do not cover the right key", f + "/on")
        columns = list(schema.columns)
        for n, t in spec_columns(rspec):
            added = f"{right}.{n}"
            if added in types:
                raise CheckError("E6002", f"column {added!r} is already present", f + "/right")
            columns.append((added, t))
        return Schema(columns, schema.key)
    raise CheckError("E6002", f"unknown op {name!r}", f + "/op")


def aggregate_type(fn, t, field):
    if fn == "count":
        return "int"
    if fn == "sum":
        if t not in NUMBER_TYPES:
            raise CheckError("E6004", f"sum of {t}", field)
        return t
    if fn in ("mean", "median"):
        if t not in NUMBER_TYPES:
            raise CheckError("E6004", f"{fn} of {t}", field)
        return "dec"
    if t == "bool":
        raise CheckError("E6004", f"{fn} of bool", field)
    return t


# --- execution --------------------------------------------------------------


def run(steps, parsed, load_table):
    """Run the pipeline. `load_table(name)` returns a Table in key order.
    Returns (list of {rows, description}, final Table)."""
    table = load_table(steps["from"])
    results = []
    for i, op in enumerate(steps["ops"]):
        table, description = step(i, op, parsed.get(i), table, load_table)
        results.append({"rows": len(table.rows), "description": description})
    return results, table


def step(i, op, expr, table, load_table):
    name = op["op"]
    f = f"/ops/{i}"
    if name == "filter":
        bound = exprs.bind(expr, table.schema)
        kept = []
        for row in table.rows:
            try:
                v = exprs.evaluate(bound, row)
            except CheckError as e:
                raise e.at(f + "/where")
            if v.t == "bool" and v.v is True:
                kept.append(row)
        out = Table(table.columns, table.key, kept)
        return out, describe.filter_(expr, len(kept), len(table.rows))
    if name == "select":
        keep = set(op["columns"]) | set(table.key)
        columns = [c for c in table.columns if c[0] in keep]
        rows = [{n: row[n] for n, _ in columns} for row in table.rows]
        return Table(columns, table.key, rows), describe.select(op, len(rows))
    if name == "sort":
        by = [(e["column"], e["dir"]) for e in op["by"]]
        rows = sort_rows(table.rows, by, op["missing"], table.key)
        return Table(table.columns, table.key, rows), describe.sort(op, table.key, len(rows))
    if name == "take":
        rows = table.rows[: op["n"]]
        return Table(table.columns, table.key, rows), describe.take(op, len(rows))
    if name == "rank":
        rows = rank_rows(op, table)
        out = Table(table.columns + [("rank", "int")], table.key, rows)
        return out, describe.rank(op, len(rows))
    if name == "derive":
        result_type = exprs.infer(expr, table.schema)
        bound = exprs.bind(expr, table.schema)
        rows = []
        for row in table.rows:
            try:
                v = exprs.evaluate(bound, row, result_type)
            except CheckError as e:
                raise e.at(f + "/expr")
            new = dict(row)
            new[op["as"]] = v
            rows.append(new)
        out = Table(table.columns + [(op["as"], result_type)], table.key, rows)
        return out, describe.derive(op, expr, len(rows))
    if name == "group":
        out = group_rows(op, table, f)
        return out, describe.group(op, len(out.rows), len(table.rows))
    if name == "join":
        right = load_table(op["right"])
        out = join_rows(op, table, right)
        return out, describe.join(op, len(out.rows))
    raise CheckError("E6002", f"unknown op {name!r}", f + "/op")


# --- order ------------------------------------------------------------------


def _cmp_values(a, b, direction, missing):
    """Compare two cells: absent cells placed per `missing` (first or last,
    whatever `direction`); present cells by value, reversed for desc."""
    aa, ab = a.absent, b.absent
    if aa and ab:
        return 0
    if aa or ab:
        c = -1 if aa else 1
        return -c if missing == "last" else c
    c = compare_present(a, b)
    return -c if direction == "desc" else c


def _cmp_key(r1, r2, key):
    for k in key:
        c = _cmp_values(r1[k], r2[k], "asc", "first")
        if c:
            return c
    return 0


def sort_rows(rows, by, missing, key):
    """Stable sort by the `by` entries [(column, dir)], then the key
    ascending with a missing component first (11.18.2 sort)."""

    def cmp(r1, r2):
        for col, direction in by:
            c = _cmp_values(r1[col], r2[col], direction, missing)
            if c:
                return c
        return _cmp_key(r1, r2, key)

    return sorted(rows, key=functools.cmp_to_key(cmp))


# --- rank -------------------------------------------------------------------


def rank_rows(op, table):
    by, direction, ties, missing = op["by"], op["dir"], op["ties"], op["missing"]
    key = table.key
    if op["scope"] == "group":
        partitions = {}
        for row in table.rows:
            k = tuple(identity(row[g]) for g in op["group_by"])
            partitions.setdefault(k, []).append(row)
        groups = list(partitions.values())
    else:
        groups = [list(table.rows)]
    ranked = []
    for members in groups:
        if missing == "exclude":
            members = [r for r in members if not r[by].absent]
        placement = "first" if missing == "first" else "last"
        ordered = sort_rows(members, [(by, direction)], placement, key)
        n = len(ordered)
        # tie groups: consecutive rows whose by values are equal (or both absent)
        starts = []
        group_index = []
        start = 0
        for idx in range(n):
            if idx > 0 and _cmp_values(ordered[idx - 1][by], ordered[idx][by], direction, placement) != 0:
                start = idx
                starts.append(start)
            elif idx == 0:
                starts.append(0)
            group_index.append(len(starts) - 1)
        for idx, row in enumerate(ordered):
            g = group_index[idx]
            first = starts[g]
            last = (starts[g + 1] - 1) if g + 1 < len(starts) else n - 1
            if ties == "min":
                r = first + 1
            elif ties == "max":
                r = last + 1
            elif ties == "dense":
                r = g + 1
            else:
                r = idx + 1
            new = dict(row)
            new["rank"] = Value("int", r)
            ranked.append(new)
    return sort_rows(ranked, [("rank", "asc")], "first", key)


# --- group ------------------------------------------------------------------


def _cmp_tuple(t1, t2):
    for a, b in zip(t1, t2):
        c = _cmp_values(a, b, "asc", "first")
        if c:
            return c
    return 0


def group_rows(op, table, field):
    by = op["by"]
    groups = {}
    order = []
    for row in table.rows:
        k = tuple(identity(row[b]) for b in by)
        if k not in groups:
            values = [MISSING if row[b].absent else row[b] for b in by]
            groups[k] = (values, [])
            order.append(k)
        groups[k][1].append(row)
    order.sort(key=functools.cmp_to_key(lambda a, b: _cmp_tuple(groups[a][0], groups[b][0])))
    types = table.schema
    columns = [(b, types[b]) for b in by]
    for a in op["aggregates"]:
        columns.append((a["as"], aggregate_type(a["fn"], types[a["column"]], None)))
    columns += [("member_count", "int"), ("member_keys_hash", "str")]
    rows = []
    for k in order:
        values, members = groups[k]
        out = dict(zip(by, values))
        for j, a in enumerate(op["aggregates"]):
            cells = [m[a["column"]] for m in members]
            try:
                out[a["as"]] = aggregate(a["fn"], cells)
            except CheckError as e:
                raise e.at(f"{field}/aggregates/{j}")
        out["member_count"] = Value("int", len(members))
        tuples = [[v.to_json() for v in table.key_tuple(m)] for m in members]
        tuples.sort(key=lambda t: canonical(t).encode("utf-8"))
        out["member_keys_hash"] = Value("str", id_of(tuples))
        rows.append(out)
    return Table(columns, list(by), rows)


def aggregate(fn, cells):
    if fn == "count":
        return Value("int", len(cells))
    present = [c for c in cells if not c.absent]
    if not present:
        return MISSING
    if fn == "sum":
        if all(c.t == "int" for c in present):
            # Each partial sum is bounded, as for dec (ruling 57).
            total = present[0]
            for c in present[1:]:
                total = exprs.int_result(total.v + c.v)
            return total
        total = as_decimal(present[0])
        try:
            for c in present[1:]:
                total = CONTEXT.add(total, as_decimal(c))
        except decimal.Overflow:
            raise CheckError("E5002", "arithmetic overflow in sum") from None
        return Value("dec", total)
    if fn in ("min", "max"):
        best = present[0]
        for c in present[1:]:
            d = compare_present(c, best)
            if (fn == "min" and d < 0) or (fn == "max" and d > 0):
                best = c
        return best
    doubles = [float(as_decimal(c)) for c in present]
    if fn == "mean":
        total = 0.0
        for x in doubles:
            total += x
        r = total / len(doubles)
    else:
        doubles.sort()
        n = len(doubles)
        rn = (n - 1) * 0.5
        lo, hi = math.floor(rn), math.ceil(rn)
        if lo == hi:
            r = doubles[lo]
        else:
            r = doubles[lo] + (doubles[hi] - doubles[lo]) * (rn - lo)
    d = dec_from_double(r)
    return MISSING if d is None else Value("dec", d)


# --- join -------------------------------------------------------------------


def join_rows(op, left, right):
    pairs = [(p["left"], p["right"]) for p in op["on"]]
    prefix = op["right"] + "."
    columns = left.columns + [(prefix + n, t) for n, t in right.columns]
    rows = []
    for row in left.rows:
        matches = [r for r in right.rows if all(equal(row[l], r[rc]) for l, rc in pairs)]
        if not matches:
            if op["kind"] == "inner":
                continue
            new = dict(row)
            for n, _ in right.columns:
                new[prefix + n] = MISSING
            rows.append(new)
            continue
        for m in matches:
            new = dict(row)
            for n, _ in right.columns:
                new[prefix + n] = m[n]
            rows.append(new)
    return Table(columns, left.key, rows)
