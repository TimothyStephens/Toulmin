"""Event IDs, genesis strings, and the envelope line form (design 11.2, item 88).

Mirrors crates/tm-formats/src/id.rs and the line-form part of envelope.rs.
"""

import hashlib

from canonical_json import FormatError, canonical, canonical_bytes, parse

PREFIX = "sha256:"


def derive_id(canonical_data):
    """`"sha256:" + hex(sha256(bytes))` over canonical bytes."""
    return PREFIX + hashlib.sha256(canonical_data).hexdigest()


def id_of(value):
    """The ID of a parsed JSON value: the hash of its canonical bytes."""
    return derive_id(canonical_bytes(value))


def genesis(chain, project_id):
    """The `prev` of a chain's first event."""
    return id_of({"chain": chain, "project_id": project_id})


def line_of(envelope):
    """The line for an envelope without `id`: its canonical text with
    `,"id":"<id>"` inserted before the final `}`. Returns (line, id)."""
    if "id" in envelope:
        raise ValueError("envelope already carries id")
    text = canonical(envelope)
    event_id = id_of(envelope)
    return text[:-1] + ',"id":"' + event_id + '"}', event_id


def read_line(line):
    """Check a line's form and ID. Returns (envelope without id, id).

    Raises FormatError: E1003 or E1001 if the line is not acceptable JSON,
    E1002 at /id if `id` is absent or not a string, the line is not in the
    exact form `line_of` writes, or the ID does not match.
    """
    value = parse(line)
    if not isinstance(value, dict):
        raise FormatError("E1002", "envelope is not an object", "")
    event_id = value.pop("id", None)
    if not isinstance(event_id, str):
        raise FormatError("E1002", "envelope has no id", "/id")
    expected_line, expected_id = line_of(value)
    if event_id != expected_id:
        raise FormatError("E1002", "id does not match the envelope", "/id")
    if line != expected_line:
        raise FormatError("E1002", "line is not in canonical form", "/id")
    return value, event_id
