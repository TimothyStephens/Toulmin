"""Typed values (checker.md 11.6) and the decimal context (11.6, 11.18.2).

A `Value` carries `t` (int, dec, str, bool, date, datetime, missing, nan,
inf), `v` (Python int, Decimal, NFC str, bool, or the RFC 3339 text of a
date or datetime) and `s` (the sign of an inf).

Decimal arithmetic runs in CONTEXT, IEEE 754-2008 decimal128 (ruling 56):
precision 34, Emax 6144, Emin -6143, ROUND_HALF_EVEN; only Overflow is
trapped (E5002). Exact
conversions (a literal to positional notation, a cell to its column scale,
tolerance margins) run in EXACT, a context with the largest precision the
module allows, so they never round.
"""

import datetime as _dt
import decimal
import math
import re
import unicodedata
from decimal import Decimal

from errors import CheckError

CONTEXT = decimal.Context(
    prec=34,
    Emax=6144,
    Emin=-6143,
    rounding=decimal.ROUND_HALF_EVEN,
    traps=[decimal.Overflow],
    flags=[],
)

# Exact arithmetic for conversions that must not round (11.18.1).
EXACT = decimal.Context(
    prec=decimal.MAX_PREC,
    Emax=decimal.MAX_EMAX,
    Emin=decimal.MIN_EMIN,
    rounding=decimal.ROUND_HALF_EVEN,
    traps=[],
    flags=[],
)

EMAX = 6144
EMIN = -6143

INT_RE = re.compile(r"-?(0|[1-9][0-9]*)")
DEC_RE = re.compile(r"-?(0|[1-9][0-9]*)(\.[0-9]+)?")
DATE_RE = re.compile(r"([0-9]{4})-([0-9]{2})-([0-9]{2})")
DATETIME_RE = re.compile(
    r"([0-9]{4})-([0-9]{2})-([0-9]{2})T([0-9]{2}):([0-9]{2}):([0-9]{2})"
    r"(?:\.([0-9]{1,9}))?Z"
)

NUMBER_TYPES = ("int", "dec")
ABSENT_TYPES = ("missing", "nan", "inf")
COLUMN_TYPES = ("int", "dec", "str", "bool", "date", "datetime")


class Value:
    __slots__ = ("t", "v", "s")

    def __init__(self, t, v=None, s=None):
        self.t = t
        self.v = v
        self.s = s

    @property
    def absent(self):
        """missing, nan, or inf: `missing` for every operator (11.18.2)."""
        return self.t in ABSENT_TYPES

    @property
    def is_number(self):
        return self.t in NUMBER_TYPES

    def to_json(self):
        if self.t == "missing" or self.t == "nan":
            return {"t": self.t}
        if self.t == "inf":
            return {"t": "inf", "s": self.s}
        if self.t == "int":
            return {"t": "int", "v": str(self.v)}
        if self.t == "dec":
            return {"t": "dec", "v": format_dec(self.v)}
        return {"t": self.t, "v": self.v}

    def __repr__(self):
        return f"Value({self.to_json()})"


MISSING = Value("missing")


def nfc(s):
    return unicodedata.normalize("NFC", s)


# --- decimal helpers --------------------------------------------------------


def to_positional(d):
    """The same number with a non-positive exponent: `1E+3` becomes `1000`.
    Exact, outside any context (11.18.1)."""
    sign, digits, exp = d.as_tuple()
    if exp > 0:
        return Decimal((sign, digits + (0,) * exp, 0))
    return d


def format_dec(d):
    """The `dec` `v` form: positional, negative zero without its sign, scale
    kept (11.6)."""
    if d.is_zero():
        d = d.copy_abs()
    return format(to_positional(d), "f")


def scale_of(d):
    """Digits after the point once written positionally, minimum 0."""
    exp = d.as_tuple().exponent
    return max(0, -exp)


def quantize_exact(d, scale):
    """`d` written at `scale`, computed exactly. Returns (quantized, changed)."""
    q = d.quantize(Decimal((0, (1,), -scale)), context=EXACT)
    return q, q != d


def strip_zeros(d):
    """Shortest digits for the same value (trailing zeros removed), exact."""
    if d.is_zero():
        return Decimal(0)
    return d.normalize(EXACT)


def dec_from_double(x):
    """A `dec` from an IEEE double (11.6): rounded to 15 significant digits
    from the exact binary value, then the shortest digits that round-trip at
    that precision, positional. None when `x` is not finite."""
    if not math.isfinite(x):
        return None
    d = Decimal(format(x, ".15g"))
    return to_positional(strip_zeros(d))


def dec_from_double_shortest(x):
    """A `float64` cell: the shortest round-trip decimal string of the IEEE
    value, positional, either zero written `0` (11.6). `repr` gives the
    shortest digits but writes an integral double as `7.0`; the trailing
    zeros of the digit string are not part of the shortest form, so they
    are dropped (`7`, not `7.0`; checker-diffs D2)."""
    if x == 0:
        return Decimal(0)
    sign, digits, exp = Decimal(repr(x)).as_tuple()
    while len(digits) > 1 and digits[-1] == 0:
        digits, exp = digits[:-1], exp + 1
    return to_positional(Decimal((sign, digits, exp)))


def as_decimal(value):
    """The Decimal of a number Value (int or dec)."""
    if value.t == "int":
        return Decimal(value.v)
    return value.v


def dec_value(d):
    return Value("dec", d)


def int_value(i):
    return Value("int", i)


# --- lexical checks on typed values (11.6, E1004) ---------------------------


def _bad(field, msg):
    return CheckError("E1004", msg, field)


def parse_typed(obj, field, allow_list=True):
    """Check the lexical form of a typed value (11.6) and return a Value.
    `field` is the JSON Pointer of the typed value; errors point below it."""
    if not isinstance(obj, dict):
        raise _bad(field, "typed value is not an object")
    t = obj.get("t")
    if not isinstance(t, str):
        raise _bad(field + "/t", "typed value has no t")
    keys = set(obj)
    if t in ("missing", "nan"):
        if keys != {"t"}:
            raise _bad(field, f"{t} takes exactly t")
        return Value(t)
    if t == "inf":
        if keys != {"t", "s"}:
            raise _bad(field, "inf takes exactly t and s")
        if obj["s"] not in ("+", "-"):
            raise _bad(field + "/s", "s of inf is + or -")
        return Value("inf", None, obj["s"])
    if t == "list":
        if not allow_list:
            raise _bad(field + "/t", "list is not allowed here")
        if keys != {"t", "v"} or not isinstance(obj["v"], list):
            raise _bad(field, "list takes exactly t and v, v a list")
        items = [parse_typed(x, f"{field}/v/{i}", allow_list=False) for i, x in enumerate(obj["v"])]
        return Value("list", items)
    if t not in COLUMN_TYPES:
        raise _bad(field + "/t", f"unknown typed value type {t!r}")
    if keys != {"t", "v"}:
        raise _bad(field, f"{t} takes exactly t and v")
    v = obj["v"]
    vf = field + "/v"
    if t == "bool":
        if not isinstance(v, bool):
            raise _bad(vf, "bool v is true or false")
        return Value("bool", v)
    if not isinstance(v, str):
        raise _bad(vf, f"{t} v is a string")
    if t == "int":
        if not INT_RE.fullmatch(v):
            raise _bad(vf, f"int v {v!r} is not -?(0|[1-9][0-9]*)")
        if v.startswith("-") and set(v[1:]) == {"0"}:
            raise _bad(vf, "minus sign before zero")
        return Value("int", int(v))
    if t == "dec":
        if not DEC_RE.fullmatch(v):
            raise _bad(vf, f"dec v {v!r} is not positional")
        if v.startswith("-") and set(v[1:]) <= {"0", "."}:
            raise _bad(vf, "minus sign before zero")
        return Value("dec", Decimal(v))
    if t == "str":
        return Value("str", nfc(v))
    if t == "date":
        if not valid_date(v):
            raise _bad(vf, f"date v {v!r} is not a valid YYYY-MM-DD")
        return Value("date", v)
    if t == "datetime":
        if not valid_datetime(v):
            raise _bad(vf, f"datetime v {v!r} is not YYYY-MM-DDTHH:MM:SS[.f]Z")
        return Value("datetime", v)
    raise _bad(field + "/t", f"unknown typed value type {t!r}")


def valid_date(v):
    m = DATE_RE.fullmatch(v)
    if not m:
        return False
    y, mo, d = (int(g) for g in m.groups())
    if y < 1:
        return False
    try:
        _dt.date(y, mo, d)
    except ValueError:
        return False
    return True


def valid_datetime(v):
    m = DATETIME_RE.fullmatch(v)
    if not m:
        return False
    y, mo, d, hh, mm, ss, frac = m.groups()
    if not valid_date(f"{y}-{mo}-{d}"):
        return False
    if int(hh) > 23 or int(mm) > 59 or int(ss) > 59:
        return False
    if frac is not None and frac.endswith("0"):
        return False
    return True


# --- equality, grouping identity, and order ---------------------------------


def datetime_key(v):
    """A text whose lexical order is chronological for datetime `v`."""
    frac = v[20:-1] if len(v) > 20 else ""
    return v[:19] + "." + frac.ljust(9, "0")


def order_key(value):
    """A key for a present value: equal values give equal keys, and the keys
    order as 11.18.2 orders the values. Only values of one comparison class
    (numbers, str, bool, date, datetime) may be compared with each other."""
    t = value.t
    if t in NUMBER_TYPES:
        return as_decimal(value)
    if t == "str":
        return value.v.encode("utf-8")
    if t == "bool":
        return 1 if value.v else 0
    if t == "date":
        return value.v
    if t == "datetime":
        return datetime_key(value.v)
    raise TypeError(f"no order for {t}")


def identity(value):
    """Grouping identity (11.18.2 group): every missing, nan, or inf value is
    one identity; present values are one identity when equal under `=`."""
    if value.absent:
        return ("missing",)
    return (comparison_class(value.t), order_key(value))


def comparison_class(t):
    if t in NUMBER_TYPES:
        return "number"
    return t


def types_compare(ta, tb):
    """Whether two column types compare under `=` (11.18.2 Types)."""
    return comparison_class(ta) == comparison_class(tb)


def equal(a, b):
    """The `=` rule: never true when either side is absent."""
    if a.absent or b.absent:
        return False
    return identity(a) == identity(b)


def compare_present(a, b):
    """-1, 0, 1 for two present values of one comparison class."""
    ka, kb = order_key(a), order_key(b)
    if ka < kb:
        return -1
    if ka > kb:
        return 1
    return 0
