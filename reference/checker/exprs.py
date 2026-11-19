"""The expression grammar (checker.md 11.18.1): tokens, parser, static type
check (11.18.2 Types), evaluation (11.18.2), and re-serialization for the
description templates (11.18.4 `{expr}`).

Nodes are tuples:
  ("lit", kind, text, value)   kind in int, dec, str, bool, missing
  ("col", name)
  ("call", name, [args])
  ("neg", operand)
  ("not", operand)
  ("bin", op, left, right)     op in + - * / = != < <= > >= and or
"""

import decimal
import math
import re
from decimal import Decimal

from errors import CheckError
from typed_values import (
    CONTEXT,
    EMAX,
    EMIN,
    MISSING,
    NUMBER_TYPES,
    Value,
    as_decimal,
    compare_present,
    dec_from_double,
    equal,
    nfc,
    to_positional,
)

IDENT_RE = re.compile(r"[A-Za-z_][A-Za-z0-9_]*")
NUMBER_RE = re.compile(r"[0-9]+(\.[0-9]+)?([eE][+-]?[0-9]+)?")
RESERVED = {"and", "or", "not", "true", "false", "missing"}
CALLS = {"abs", "log", "log2", "log10", "exp", "coalesce"}
CMP_OPS = {"=", "!=", "<", "<=", ">", ">="}

# Precedence levels for re-serialization (higher binds tighter).
PREC_OR, PREC_AND, PREC_NOT, PREC_CMP, PREC_ADD, PREC_MUL, PREC_UNARY, PREC_PRIMARY = range(1, 9)


def syntax(msg):
    return CheckError("E6006", f"expression syntax error: {msg}")


# --- tokens -----------------------------------------------------------------


def tokenize(text):
    """Yields (kind, text) with kind in NUM, STR, IDENT, COL (backticked), OP."""
    tokens = []
    i, n = 0, len(text)
    while i < n:
        c = text[i]
        if c in " \t\r\n":
            i += 1
            continue
        if c.isdigit():
            m = NUMBER_RE.match(text, i)
            tokens.append(("NUM", m.group(0)))
            i = m.end()
            continue
        if c == '"':
            j = i + 1
            while True:
                if j >= n:
                    raise syntax("unterminated string")
                if text[j] == "\\":
                    if j + 1 >= n or text[j + 1] not in '"\\':
                        raise syntax("bad escape in string")
                    j += 2
                    continue
                if text[j] == '"':
                    break
                j += 1
            tokens.append(("STR", text[i : j + 1]))
            i = j + 1
            continue
        if c == "`":
            j = text.find("`", i + 1)
            if j < 0:
                raise syntax("unterminated backtick name")
            if j == i + 1:
                raise syntax("empty backtick name")
            tokens.append(("COL", text[i + 1 : j]))
            i = j + 1
            continue
        m = IDENT_RE.match(text, i)
        if m:
            tokens.append(("IDENT", m.group(0)))
            i = m.end()
            continue
        two = text[i : i + 2]
        if two in ("!=", "<=", ">="):
            tokens.append(("OP", two))
            i += 2
            continue
        if c in "=<>+-*/(),":
            tokens.append(("OP", c))
            i += 1
            continue
        raise syntax(f"unexpected character {c!r}")
    return tokens


# --- parser -----------------------------------------------------------------


class Parser:
    def __init__(self, text):
        self.tokens = tokenize(text)
        self.pos = 0

    def peek(self):
        return self.tokens[self.pos] if self.pos < len(self.tokens) else (None, None)

    def take(self):
        tok = self.peek()
        self.pos += 1
        return tok

    def is_op(self, *texts):
        kind, text = self.peek()
        return kind == "OP" and text in texts

    def is_word(self, word):
        kind, text = self.peek()
        return kind == "IDENT" and text == word

    def expect_op(self, text):
        kind, got = self.take()
        if kind != "OP" or got != text:
            raise syntax(f"expected {text!r}")

    def parse(self):
        node = self.p_or()
        if self.peek()[0] is not None:
            raise syntax(f"unexpected token {self.peek()[1]!r}")
        return node

    def p_or(self):
        node = self.p_and()
        while self.is_word("or"):
            self.take()
            node = ("bin", "or", node, self.p_and())
        return node

    def p_and(self):
        node = self.p_not()
        while self.is_word("and"):
            self.take()
            node = ("bin", "and", node, self.p_not())
        return node

    def p_not(self):
        if self.is_word("not"):
            self.take()
            return ("not", self.p_not())
        return self.p_cmp()

    def p_cmp(self):
        node = self.p_add()
        kind, text = self.peek()
        if kind == "OP" and text in CMP_OPS:
            self.take()
            node = ("bin", text, node, self.p_add())
        return node

    def p_add(self):
        node = self.p_mul()
        while self.is_op("+", "-"):
            op = self.take()[1]
            node = ("bin", op, node, self.p_mul())
        return node

    def p_mul(self):
        node = self.p_unary()
        while self.is_op("*", "/"):
            op = self.take()[1]
            node = ("bin", op, node, self.p_unary())
        return node

    def p_unary(self):
        if self.is_op("-"):
            self.take()
            return ("neg", self.p_unary())
        return self.p_primary()

    def p_primary(self):
        kind, text = self.take()
        if kind == "NUM":
            return number_literal(text)
        if kind == "STR":
            return ("lit", "str", text, nfc(unescape(text)))
        if kind == "COL":
            return ("col", text)
        if kind == "IDENT":
            if text == "true":
                return ("lit", "bool", text, True)
            if text == "false":
                return ("lit", "bool", text, False)
            if text == "missing":
                return ("lit", "missing", text, None)
            if text in RESERVED:
                raise syntax(f"reserved word {text!r}")
            if self.is_op("("):
                if text not in CALLS:
                    raise syntax(f"unknown function {text!r}")
                self.take()
                args = [self.p_or()]
                while self.is_op(","):
                    self.take()
                    args.append(self.p_or())
                self.expect_op(")")
                if text == "coalesce":
                    if len(args) < 2:
                        raise syntax("coalesce takes two or more arguments")
                elif len(args) != 1:
                    raise syntax(f"{text} takes one argument")
                return ("call", text, args)
            return ("col", text)
        if kind == "OP" and text == "(":
            node = self.p_or()
            self.expect_op(")")
            return node
        if kind is None:
            raise syntax("unexpected end of expression")
        raise syntax(f"unexpected token {text!r}")


def unescape(token):
    body = token[1:-1]
    return body.replace('\\"', '"').replace("\\\\", "\\")


def number_literal(text):
    """INT, DEC, or EXPNUM. An EXPNUM is a dec converted exactly to positional
    notation; its adjusted exponent must lie within the context (E6004)."""
    if "e" in text or "E" in text:
        d = Decimal(text)
        if d.adjusted() > EMAX or d.adjusted() < EMIN:
            raise CheckError(
                "E6004", f"literal {text} has an adjusted exponent outside the context"
            )
        return ("lit", "dec", text, to_positional(d))
    if "." in text:
        return ("lit", "dec", text, Decimal(text))
    return ("lit", "int", text, int(text))


def parse(text):
    """Parse an expression string. Raises CheckError E6006 (syntax) or E6004
    (literal out of range) with `field` unset."""
    return Parser(text).parse()


# --- static types (11.18.2 Types) -------------------------------------------


def type_error(msg):
    return CheckError("E6004", f"operand type error: {msg}")


def infer(node, schema):
    """The static type of `node` over `schema` (dict name -> type), or None
    for an expression that has no type (the literal `missing`). Raises
    E6005 for an unknown column and E6004 for an operand type error."""
    kind = node[0]
    if kind == "lit":
        return None if node[1] == "missing" else node[1]
    if kind == "col":
        name = node[1]
        if name not in schema:
            raise CheckError("E6005", f"unknown column {name!r}")
        t = schema[name]
        return "dec" if t == "float64" else t
    if kind == "neg":
        t = infer(node[1], schema)
        if t is None or t in NUMBER_TYPES:
            return t
        raise type_error(f"unary - on {t}")
    if kind == "not":
        t = infer(node[1], schema)
        if t is None or t == "bool":
            return "bool"
        raise type_error(f"not on {t}")
    if kind == "call":
        name, args = node[1], node[2]
        types = [infer(a, schema) for a in args]
        if name == "coalesce":
            common = None
            for t in types:
                if t is None:
                    continue
                if common is None:
                    common = t
                elif common == t:
                    continue
                elif common in NUMBER_TYPES and t in NUMBER_TYPES:
                    common = "dec"
                else:
                    raise type_error(f"coalesce of {common} and {t}")
            return common
        t = types[0]
        if t is not None and t not in NUMBER_TYPES:
            raise type_error(f"{name} on {t}")
        if name == "abs":
            return t
        return "dec"
    op, left, right = node[1], node[2], node[3]
    lt, rt = infer(left, schema), infer(right, schema)
    if op in ("and", "or"):
        for t in (lt, rt):
            if t is not None and t != "bool":
                raise type_error(f"{op} on {t}")
        return "bool"
    if op in CMP_OPS:
        if lt is None or rt is None:
            return "bool"
        if lt in NUMBER_TYPES and rt in NUMBER_TYPES:
            return "bool"
        if lt != rt:
            raise type_error(f"{lt} {op} {rt}")
        if lt == "bool" and op not in ("=", "!="):
            raise type_error(f"{op} on bool")
        return "bool"
    # arithmetic
    for t in (lt, rt):
        if t is not None and t not in NUMBER_TYPES:
            raise type_error(f"{op} on {t}")
    if op == "/":
        return "dec"
    if lt is None:
        return rt
    if rt is None:
        return lt
    return "int" if lt == "int" and rt == "int" else "dec"


# --- evaluation (11.18.2) ---------------------------------------------------


def evaluate(node, row, result_type=None):
    """Evaluate over `row` (dict name -> Value). `result_type` is the static
    type of the whole expression; a number result is widened to it (an int
    widened to dec has scale 0). Raises E5002 on decimal Overflow."""
    value = _eval(node, row)
    if result_type == "dec" and value.t == "int":
        return Value("dec", Decimal(value.v))
    return value


def _eval(node, row):
    kind = node[0]
    if kind == "lit":
        k = node[1]
        if k == "missing":
            return MISSING
        return Value(k, node[3])
    if kind == "col":
        return row[node[1]]
    if kind == "neg":
        v = _eval(node[1], row)
        if v.absent:
            return MISSING
        if v.t == "int":
            return Value("int", -v.v)
        return Value("dec", v.v.copy_negate())
    if kind == "not":
        v = _eval(node[1], row)
        if v.absent:
            return MISSING
        return Value("bool", not v.v)
    if kind == "call":
        return _call(node[1], node[2], row)
    op, left, right = node[1], node[2], node[3]
    if op in ("and", "or"):
        lv, rv = _eval(left, row), _eval(right, row)
        decisive = op == "or"  # true decides an or, false decides an and
        if (lv.t == "bool" and lv.v is decisive) or (rv.t == "bool" and rv.v is decisive):
            return Value("bool", decisive)
        if lv.absent or rv.absent:
            return MISSING
        return Value("bool", not decisive)
    lv, rv = _eval(left, row), _eval(right, row)
    if lv.absent or rv.absent:
        return MISSING
    if op in CMP_OPS:
        return Value("bool", _compare(op, lv, rv))
    return arith(op, lv, rv)


def _compare(op, a, b):
    if op == "=":
        return equal(a, b)
    if op == "!=":
        return not equal(a, b)
    c = compare_present(a, b)
    return {"<": c < 0, "<=": c <= 0, ">": c > 0, ">=": c >= 0}[op]


def arith(op, a, b):
    """+ - * / on two present numbers; both int gives int except for /."""
    if a.t == "int" and b.t == "int" and op != "/":
        if op == "+":
            return Value("int", a.v + b.v)
        if op == "-":
            return Value("int", a.v - b.v)
        return Value("int", a.v * b.v)
    da, db = as_decimal(a), as_decimal(b)
    try:
        if op == "+":
            r = CONTEXT.add(da, db)
        elif op == "-":
            r = CONTEXT.subtract(da, db)
        elif op == "*":
            r = CONTEXT.multiply(da, db)
        else:
            if db.is_zero():
                return MISSING
            r = CONTEXT.divide(da, db)
    except decimal.Overflow:
        raise CheckError("E5002", "arithmetic overflow") from None
    return Value("dec", r)


def _call(name, args, row):
    if name == "coalesce":
        for a in args:
            v = _eval(a, row)
            if not v.absent:
                return v
        return MISSING
    v = _eval(args[0], row)
    if v.absent:
        return MISSING
    if name == "abs":
        if v.t == "int":
            return Value("int", abs(v.v))
        return Value("dec", v.v.copy_abs())
    return log_family(name, v)


def log_family(name, v):
    """log, log2, log10, exp through IEEE double, rounded to 15 significant
    digits (11.18.2 `log` family). Domain errors give missing."""
    d = as_decimal(v)
    try:
        x = float(d)
    except OverflowError:
        x = math.inf if d > 0 else -math.inf
    try:
        if name == "exp":
            r = math.exp(x)
        else:
            if x <= 0:
                return MISSING
            r = {"log": math.log, "log2": math.log2, "log10": math.log10}[name](x)
    except (OverflowError, ValueError):
        return MISSING
    out = dec_from_double(r)
    if out is None:
        return MISSING
    return Value("dec", out)


# --- re-serialization (11.18.4 {expr}) --------------------------------------


def needs_backticks(name):
    return not IDENT_RE.fullmatch(name) or name in RESERVED


def render_column(name):
    return f"`{name}`" if needs_backticks(name) else name


def precedence(node):
    kind = node[0]
    if kind in ("lit", "col", "call"):
        return PREC_PRIMARY
    if kind == "neg":
        return PREC_UNARY
    if kind == "not":
        return PREC_NOT
    op = node[1]
    if op == "or":
        return PREC_OR
    if op == "and":
        return PREC_AND
    if op in CMP_OPS:
        return PREC_CMP
    if op in ("+", "-"):
        return PREC_ADD
    return PREC_MUL


def render(node):
    kind = node[0]
    if kind == "lit":
        return node[2]
    if kind == "col":
        return render_column(node[1])
    if kind == "call":
        return node[1] + "(" + ", ".join(render(a) for a in node[2]) + ")"
    if kind == "neg":
        return "-" + _wrap(node[1], PREC_UNARY)
    if kind == "not":
        return "not " + _wrap(node[1], PREC_NOT)
    op, left, right = node[1], node[2], node[3]
    level = precedence(node)
    # Left-associative: the right operand needs parentheses at equal
    # precedence to keep the parse tree; a comparison does not chain.
    lhs = _wrap(left, level if op not in CMP_OPS else level + 1)
    rhs = _wrap(right, level + 1)
    return f"{lhs} {op} {rhs}"


def _wrap(node, minimum):
    text = render(node)
    if precedence(node) < minimum:
        return "(" + text + ")"
    return text
