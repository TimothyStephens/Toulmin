"""RFC 8785 (JCS) canonical JSON, the second implementation (design 11.1, item 88).

Standard library only. Mirrors crates/tm-formats/src/canonical.rs:

* Input must be I-JSON (RFC 7493): valid UTF-8, no duplicate member names,
  no lone surrogates, no NaN or Infinity. Anything else, including any JSON
  syntax error, is E1003.
* Hashed records hold no floats. A number token with a fraction or an
  exponent, a minus zero, or an integer outside plus or minus 2**53
  (inclusive) is E1001.
* Output: members sorted by their names' UTF-16 code units, no whitespace,
  strings escaped as RFC 8785 section 3.2.2.2 requires, UTF-8 encoded.

Error precedence matches the Rust side: a float or syntax error is raised
where the scanner meets it, a duplicate name when its object closes, and a
lone surrogate only after the whole document parsed.
"""

import json

LIMIT = 2**53


class FormatError(Exception):
    """A catalog error: `code` is the stable code (E1001, E1003, ...)."""

    def __init__(self, code, message, field=None):
        super().__init__(f"{code}: {message}")
        self.code = code
        self.message = message
        self.field = field


def _parse_float(text):
    raise FormatError("E1001", f"float in canonical JSON: {text}")


def _parse_int(text):
    if text == "-0":
        raise FormatError("E1001", "minus zero in canonical JSON")
    value = int(text)
    if abs(value) > LIMIT:
        raise FormatError("E1001", f"integer outside plus or minus 2^53: {text}")
    return value


def _parse_constant(text):
    raise FormatError("E1003", f"not I-JSON: {text}")


def _pairs(pairs):
    seen = set()
    for name, _ in pairs:
        if name in seen:
            raise FormatError("E1003", f"not I-JSON: duplicate member name {name!r}")
        seen.add(name)
    return dict(pairs)


def _check_surrogates(value):
    stack = [value]
    while stack:
        v = stack.pop()
        if isinstance(v, str):
            _check_str(v)
        elif isinstance(v, list):
            stack.extend(v)
        elif isinstance(v, dict):
            for k, item in v.items():
                _check_str(k)
                stack.append(item)


def _check_str(s):
    for ch in s:
        if 0xD800 <= ord(ch) <= 0xDFFF:
            raise FormatError("E1003", "not I-JSON: lone surrogate")


def parse(data):
    """Parse bytes or str strictly. Returns plain Python values (dict, list,
    str, int, bool, None)."""
    if isinstance(data, (bytes, bytearray)):
        try:
            data = bytes(data).decode("utf-8")
        except UnicodeDecodeError as e:
            raise FormatError("E1003", f"not I-JSON: invalid UTF-8 ({e})") from None
    try:
        value = json.loads(
            data,
            parse_float=_parse_float,
            parse_int=_parse_int,
            parse_constant=_parse_constant,
            object_pairs_hook=_pairs,
        )
    except FormatError:
        raise
    except (json.JSONDecodeError, RecursionError) as e:
        raise FormatError("E1003", f"not I-JSON: {e}") from None
    _check_surrogates(value)
    return value


_ESCAPES = {
    '"': '\\"',
    "\\": "\\\\",
    "\b": "\\b",
    "\f": "\\f",
    "\n": "\\n",
    "\r": "\\r",
    "\t": "\\t",
}


def _string(s):
    _check_str(s)
    out = ['"']
    for ch in s:
        if ch in _ESCAPES:
            out.append(_ESCAPES[ch])
        elif ord(ch) < 0x20:
            out.append(f"\\u{ord(ch):04x}")
        else:
            out.append(ch)
    out.append('"')
    return "".join(out)


def _utf16_key(name):
    return name.encode("utf-16-be")


def _serialize(value, out):
    if value is None:
        out.append("null")
    elif value is True:
        out.append("true")
    elif value is False:
        out.append("false")
    elif isinstance(value, int):
        if abs(value) > LIMIT:
            raise FormatError("E1001", f"integer outside plus or minus 2^53: {value}")
        out.append(str(value))
    elif isinstance(value, float):
        raise FormatError("E1001", f"float in canonical JSON: {value!r}")
    elif isinstance(value, str):
        out.append(_string(value))
    elif isinstance(value, list):
        out.append("[")
        for i, item in enumerate(value):
            if i:
                out.append(",")
            _serialize(item, out)
        out.append("]")
    elif isinstance(value, dict):
        out.append("{")
        for i, name in enumerate(sorted(value, key=_utf16_key)):
            if i:
                out.append(",")
            out.append(_string(name))
            out.append(":")
            _serialize(value[name], out)
        out.append("}")
    else:
        raise TypeError(f"not a JSON value: {type(value).__name__}")


def canonical(value):
    """The canonical text of a parsed value (str)."""
    out = []
    _serialize(value, out)
    return "".join(out)


def canonical_bytes(value):
    """The canonical UTF-8 bytes of a parsed value."""
    return canonical(value).encode("utf-8")


def canonicalize(data):
    """Parse `data` strictly and return its canonical bytes."""
    return canonical_bytes(parse(data))
