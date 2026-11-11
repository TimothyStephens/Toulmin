"""The Python formats cross-check (design 11.1, items 88 and 90).

Run from anywhere as `python3 reference/formats/test_formats.py` (also under
`python3 -I`). Standard library only.

Checks, against conformance/formats/ (case format in
crates/tm-formats/README.md):

* canonical/: canonical text, ID, or refusal code of every input;
* ids/: every ID and genesis string;
* envelope/: every valid line reads back with the expected ID; every
  invalid line whose expected error is about the line itself (`/id`,
  `E1001`, `E1003`) is refused the same way; every other invalid line has
  a correct line form and ID, so its error is the one the Rust envelope
  validator names (the body checks are Rust's in P1);
* every case file: its own ID and its subject ID.

With TM_FORMATS_IDS_OUT set, writes `<path> <file id> <subject>` per case
file, sorted by path, the same listing `cargo test -p tm-formats --test
conformance` writes; the P1 checkpoint compares the two.
"""

import json
import os
import sys
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

from canonical_json import FormatError, canonical, parse  # noqa: E402
from ids import genesis, id_of, read_line  # noqa: E402

ROOT = os.path.normpath(os.path.join(HERE, "..", "..", "conformance", "formats"))


def case_files():
    out = []
    for dirpath, dirnames, filenames in os.walk(ROOT):
        dirnames.sort()
        for name in sorted(filenames):
            if name.endswith(".json"):
                full = os.path.join(dirpath, name)
                out.append(os.path.relpath(full, ROOT).replace(os.sep, "/"))
    return sorted(out)


def load(rel):
    with open(os.path.join(ROOT, rel), "rb") as f:
        return parse(f.read())


def id_or_code(fn):
    try:
        return fn()
    except FormatError as e:
        return e.code


def subject(kind, case):
    if kind == "canonical":
        return id_or_code(lambda: id_of(parse(case["input_text"])))
    if kind == "ids":
        if "genesis" in case:
            g = case["genesis"]
            return genesis(g["chain"], g["project_id"])
        return id_or_code(lambda: id_of(case["input"]))
    if kind == "envelope":
        def strip():
            v = parse(case["line"])
            if isinstance(v, dict):
                v.pop("id", None)
            return id_of(v)
        return id_or_code(strip)
    return id_or_code(lambda: id_of(case["input"]))


def listing():
    lines = []
    for rel in case_files():
        case = load(rel)
        kind = rel.split("/")[0]
        lines.append(f"{rel} {id_of(case)} {subject(kind, case)}\n")
    return "".join(lines)


def cases_in(kind):
    return [(rel, load(rel)) for rel in case_files() if rel.split("/")[0] == kind]


class Canonical(unittest.TestCase):
    def test_cases(self):
        cases = cases_in("canonical")
        self.assertTrue(cases)
        for rel, case in cases:
            with self.subTest(rel):
                exp = case["expect"]
                try:
                    v = parse(case["input_text"])
                    got = {"canonical": canonical(v), "id": id_of(v)}
                except FormatError as e:
                    got = {"code": e.code}
                self.assertEqual(got, exp)

    def test_rfc8785_sorting(self):
        # RFC 8785 section 3.2.3, independent of the case files.
        text = ('{"\\u20ac":"Euro Sign","\\r":"Carriage Return","\\ufb33":"Hebrew Letter Dalet With Dagesh",'
                '"1":"One","\\ud83d\\ude00":"Emoji: Grinning Face","\\u0080":"Control",'
                '"\\u00f6":"Latin Small Letter O With Diaeresis"}')
        keys = list(json.loads(canonical(parse(text))).keys())
        self.assertEqual(keys, ["\r", "1", "\u0080", "ö", "€", "\U0001F600", "דּ"])


class Ids(unittest.TestCase):
    def test_cases(self):
        cases = cases_in("ids")
        self.assertTrue(cases)
        for rel, case in cases:
            with self.subTest(rel):
                if "genesis" in case:
                    g = case["genesis"]
                    got = genesis(g["chain"], g["project_id"])
                else:
                    got = id_of(case["input"])
                self.assertEqual(got, case["expect"]["id"])


class Envelope(unittest.TestCase):
    def test_lines(self):
        cases = cases_in("envelope")
        self.assertTrue(cases)
        for rel, case in cases:
            with self.subTest(rel):
                exp = case["expect"]
                line = case["line"]
                line_level = "code" in exp and (exp.get("field") == "/id" or exp["code"] in ("E1001", "E1003"))
                if line_level:
                    with self.assertRaises(FormatError) as cm:
                        read_line(line)
                    self.assertEqual((cm.exception.code, cm.exception.field), (exp["code"], exp.get("field")))
                else:
                    _, event_id = read_line(line)
                    if "id" in exp:
                        self.assertEqual(event_id, exp["id"])


class CaseIds(unittest.TestCase):
    def test_every_case_file(self):
        text = listing()
        self.assertEqual(len(text.splitlines()), len(case_files()))
        out = os.environ.get("TM_FORMATS_IDS_OUT")
        if out:
            with open(out, "w", encoding="utf-8") as f:
                f.write(text)


if __name__ == "__main__":
    unittest.main(verbosity=2)
