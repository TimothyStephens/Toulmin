"""Parse specs (checker.md 11.7) and text tables read under them.

A `Table` has `columns` (list of (name, type) with type one of the 11.6
column types; a `float64` column is `dec`), `key` (list of column names),
and `rows` (list of dict name -> Value). Rows are in key order (11.18.2
Initial order) once `in_key_order` has run.
"""

import csv
import io
import math
import re
from decimal import Decimal

from errors import CheckError
from typed_values import (
    DEC_RE,
    INT_RE,
    MISSING,
    Value,
    dec_from_double_shortest,
    nfc,
    quantize_exact,
    to_positional,
    valid_date,
    valid_datetime,
)

SPEC_TYPES = ("int", "dec", "str", "bool", "date", "datetime", "float64")
SPEC_KEYS = {"spec", "format", "delimiter", "header", "columns", "key", "nan", "inf", "encoding"}
CELL_DEC_RE = re.compile(DEC_RE.pattern + r"([eE][+-]?[0-9]+)?")
FLOAT_RE = re.compile(r"[+-]?([0-9]+(\.[0-9]*)?|\.[0-9]+)([eE][+-]?[0-9]+)?")
NAN_TOKENS = {"nan"}
INF_TOKENS = {"inf": "+", "+inf": "+", "-inf": "-", "infinity": "+", "+infinity": "+", "-infinity": "-"}


class Table:
    def __init__(self, columns, key, rows):
        self.columns = columns  # list of (name, type)
        self.key = key
        self.rows = rows

    @property
    def names(self):
        return [n for n, _ in self.columns]

    @property
    def schema(self):
        return dict(self.columns)

    def key_tuple(self, row):
        return [row[k] for k in self.key]


# --- parse spec -------------------------------------------------------------


def _schema_error(code, field, msg):
    return CheckError(code, msg, field)


def check_spec(spec):
    """Validate a parse spec's shape (11.7): E3105 for a schema violation,
    E3103 for a key naming no column, E3104 for an encoding other than
    utf-8. Returns the spec."""
    if not isinstance(spec, dict):
        raise _schema_error("E3105", "", "parse spec is not an object")
    for k in SPEC_KEYS:
        if k not in spec:
            raise _schema_error("E3105", "/" + k, f"{k} is required")
    for k in spec:
        if k not in SPEC_KEYS:
            raise _schema_error("E3105", "/" + k, "unknown member")
    if spec["spec"] != 1:
        raise _schema_error("E3105", "/spec", "unknown spec version")
    fmt = spec["format"]
    if fmt not in ("csv", "tsv", "parquet"):
        raise _schema_error("E3105", "/format", "format is csv, tsv, or parquet")
    if fmt == "parquet":
        if spec["delimiter"] is not None or spec["header"] is not None:
            raise _schema_error("E3105", "/delimiter", "parquet takes null delimiter and header")
    else:
        if not isinstance(spec["delimiter"], str) or len(spec["delimiter"]) != 1:
            raise _schema_error("E3105", "/delimiter", "delimiter is a one-character string")
        if not isinstance(spec["header"], bool):
            raise _schema_error("E3105", "/header", "header is a boolean")
    if spec["encoding"] != "utf-8":
        raise _schema_error("E3104", "/encoding", "only utf-8 is accepted")
    for k in ("nan", "inf"):
        if spec[k] not in ("missing", "value"):
            raise _schema_error("E3105", "/" + k, f"{k} is missing or value")
    cols = spec["columns"]
    if not isinstance(cols, list) or not cols:
        raise _schema_error("E3105", "/columns", "columns is a non-empty list")
    names = []
    for i, c in enumerate(cols):
        f = f"/columns/{i}"
        if not isinstance(c, dict):
            raise _schema_error("E3105", f, "column is an object")
        for k in ("name", "type", "missing"):
            if k not in c:
                raise _schema_error("E3105", f"{f}/{k}", f"{k} is required")
        if not isinstance(c["name"], str) or not c["name"]:
            raise _schema_error("E3105", f"{f}/name", "name is a non-empty string")
        if c["name"] in names:
            raise _schema_error("E3105", f"{f}/name", "column names are distinct")
        names.append(c["name"])
        if c["type"] not in SPEC_TYPES:
            raise _schema_error("E3105", f"{f}/type", "unknown column type")
        if not isinstance(c["missing"], list) or not all(isinstance(m, str) for m in c["missing"]):
            raise _schema_error("E3105", f"{f}/missing", "missing is a list of strings")
        if c["type"] == "dec":
            s = c.get("scale")
            if not isinstance(s, int) or isinstance(s, bool) or s < 0 or s > 38:
                raise _schema_error("E3105", f"{f}/scale", "scale is an integer from 0 to 38")
            extra = set(c) - {"name", "type", "missing", "scale"}
        else:
            extra = set(c) - {"name", "type", "missing"}
        if extra:
            raise _schema_error("E3105", f"{f}/{sorted(extra)[0]}", "unknown member")
    key = spec["key"]
    if not isinstance(key, list) or not key:
        raise _schema_error("E3105", "/key", "key is a non-empty list")
    for i, k in enumerate(key):
        if not isinstance(k, str) or k in key[:i]:
            raise _schema_error("E3105", f"/key/{i}", "key names are distinct strings")
        if k not in names:
            raise _schema_error("E3103", f"/key/{i}", f"key column {k!r} is not a column")
    return spec


def spec_columns(spec):
    """(name, type) per column with float64 read as dec (11.18.2 Types)."""
    return [(c["name"], "dec" if c["type"] == "float64" else c["type"]) for c in spec["columns"]]


# --- cells ------------------------------------------------------------------


def parse_cell(text, column, spec, where):
    """One cell under its column's spec. `where` names the row and column
    for the E3102 message; a data cell has `field` null (11.7)."""
    if text in column["missing"]:
        return MISSING
    t = column["type"]

    def fail(msg):
        return CheckError("E3102", f"{where}: {msg}", None)

    if t == "str":
        return Value("str", nfc(text))
    if t == "int":
        if not INT_RE.fullmatch(text):
            raise fail(f"{text!r} is not an int")
        return Value("int", int(text))
    if t == "dec":
        if not CELL_DEC_RE.fullmatch(text):
            raise fail(f"{text!r} is not a dec")
        d = to_positional(Decimal(text))
        q, changed = quantize_exact(d, column["scale"])
        if changed:
            raise fail(f"{text!r} changes when written at scale {column['scale']}")
        if len(q.as_tuple().digits) > 38:
            raise fail(f"{text!r} needs more than 38 digits at scale {column['scale']}")
        return Value("dec", q)
    if t == "float64":
        low = text.lower()
        if low in NAN_TOKENS:
            return Value("nan") if spec["nan"] == "value" else MISSING
        if low in INF_TOKENS:
            return Value("inf", None, INF_TOKENS[low]) if spec["inf"] == "value" else MISSING
        if not FLOAT_RE.fullmatch(text):
            raise fail(f"{text!r} is not a number")
        x = float(text)
        if math.isinf(x):
            raise fail(f"{text!r} overflows a double")
        return Value("dec", dec_from_double_shortest(x))
    if t == "bool":
        if text == "true":
            return Value("bool", True)
        if text == "false":
            return Value("bool", False)
        raise fail(f"{text!r} is not a bool")
    if t == "date":
        if not valid_date(text):
            raise fail(f"{text!r} is not a date")
        return Value("date", text)
    if t == "datetime":
        if not valid_datetime(text):
            raise fail(f"{text!r} is not a datetime")
        return Value("datetime", text)
    raise fail(f"unknown column type {t}")


# --- files ------------------------------------------------------------------


def read_table(data, spec):
    """Read a csv or tsv file's bytes under `spec`. Returns a Table with
    rows in file order; call `in_key_order` for the initial order."""
    check_spec(spec)
    if spec["format"] == "parquet":
        raise CheckError("E3105", "parquet is not readable by the reference checker", "/format")
    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError as e:
        raise CheckError("E3102", f"file is not UTF-8: {e}", None) from None
    if text.startswith("﻿"):
        text = text[1:]
    reader = csv.reader(io.StringIO(text, newline=""), delimiter=spec["delimiter"])
    records = list(reader)
    columns = spec["columns"]
    names = [c["name"] for c in columns]
    if spec["header"]:
        if not records:
            raise CheckError("E3103", "file has no header", None)
        header = records[0]
        records = records[1:]
        for i, n in enumerate(names):
            if n not in header:
                raise CheckError("E3103", f"column {n!r} is not in the file header", f"/columns/{i}")
        index = {n: header.index(n) for n in names}
    else:
        index = {n: i for i, n in enumerate(names)}
    rows = []
    for r, record in enumerate(records, start=1):
        if not record:
            continue
        row = {}
        for c in columns:
            i = index[c["name"]]
            if i >= len(record):
                raise CheckError("E3102", f"row {r}: column {c['name']!r} has no cell", None)
            row[c["name"]] = parse_cell(record[i], c, spec, f"row {r}, column {c['name']!r}")
        rows.append(row)
    table = Table(spec_columns(spec), list(spec["key"]), rows)
    check_key_unique(table)
    return table


def check_key_unique(table):
    from typed_values import identity

    seen = set()
    for row in table.rows:
        k = tuple(identity(v) for v in table.key_tuple(row))
        if k in seen:
            raise CheckError("E3101", "duplicate key", None)
        seen.add(k)


def in_key_order(table):
    """Rows ascending by each key column, a missing component first."""
    from ops import sort_rows

    table.rows = sort_rows(table.rows, [], "first", table.key)
    return table
