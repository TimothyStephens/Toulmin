#!/usr/bin/env bash
# Item 8 and item 93. The four normative spec files under docs/spec/ are
# copies of named sections of docs/design/toulmin_v1_design.md and must stay
# equal to them after heading normalization; docs/errors.md must equal the
# error catalog build.rs emits.
#
# Mechanism (set here at P0, used from P1):
#   * Each docs/spec/<name>.md starts with a line
#         <!-- design-sections: 11.1-11.17, 11.21, 11.23 -->
#     naming the design sections it copies (ranges inclusive, in design
#     order). Item 93 fixes the lists: formats.md 11.1-11.17, 11.21, 11.23;
#     record.md 3, 11.21; checker.md 11.6-11.10, 11.18; claims.md 7, 11.22.
#   * A design section is the text from its numbered heading up to the next
#     heading of the same or a higher level.
#   * Heading normalization: the `#` run and the section number are dropped
#     from heading lines on both sides, and trailing whitespace and blank-line
#     runs are collapsed.
#   * docs/errors.md is compared byte for byte with the file build.rs writes
#     to $TM_ERRORS_OUT when that variable is set.
# No-op that passes until docs/spec exists (P1).
set -euo pipefail
cd "$(dirname "$0")/../.."
design=docs/design/toulmin_v1_design.md

if [ ! -d docs/spec ]; then
  echo "spec-drift: no-op, docs/spec does not exist yet (P1)"
  exit 0
fi

status=0
for spec in docs/spec/*.md; do
  if ! python3 -I - "$design" "$spec" <<'PY'; then status=1; fi
import re, sys, difflib
design_path, spec_path = sys.argv[1], sys.argv[2]
design = open(design_path, encoding="utf-8").read().splitlines()
spec = open(spec_path, encoding="utf-8").read().splitlines()
m = re.match(r"\s*<!--\s*design-sections:\s*(.*?)\s*-->\s*$", spec[0] if spec else "")
if not m:
    print(f"FAIL {spec_path}: first line must be <!-- design-sections: ... -->")
    sys.exit(1)
wanted = []
for part in m.group(1).split(","):
    part = part.strip()
    if "-" in part:
        a, b = part.split("-")
        pre = a.rsplit(".", 1)[0] + "." if "." in a else ""
        lo, hi = int(a.rsplit(".", 1)[-1]), int(b.rsplit(".", 1)[-1])
        wanted += [f"{pre}{i}" for i in range(lo, hi + 1)]
    else:
        wanted.append(part)
heading = re.compile(r"^(#+)\s+(\d+(?:\.\d+)*)\.?\s+(.*)$")
def normalize(lines):
    out = []
    for line in lines:
        h = heading.match(line)
        if h:
            line = h.group(3).strip()
        elif line.startswith("#"):
            line = line.lstrip("#").strip()
        line = line.rstrip()
        if line == "" and out and out[-1] == "":
            continue
        out.append(line)
    while out and out[-1] == "":
        out.pop()
    while out and out[0] == "":
        out.pop(0)
    return out
def section(number):
    start = None
    level = None
    for i, line in enumerate(design):
        h = heading.match(line)
        if h and h.group(2) == number:
            start, level = i, len(h.group(1))
            break
    if start is None:
        return None
    end = len(design)
    for j in range(start + 1, len(design)):
        h = re.match(r"^(#+)\s", design[j])
        if h and len(h.group(1)) <= level:
            end = j
            break
    return design[start:end]
expected = []
for num in wanted:
    body = section(num)
    if body is None:
        print(f"FAIL {spec_path}: design section {num} not found")
        sys.exit(1)
    expected += body + [""]
left = normalize(expected)
right = normalize(spec[1:])
if left != right:
    print(f"FAIL {spec_path}: differs from design sections {', '.join(wanted)}")
    sys.stdout.writelines(difflib.unified_diff(left, right, "design", spec_path, lineterm="", n=2))
    print()
    sys.exit(1)
print(f"OK {spec_path} matches {', '.join(wanted)}")
PY
done

if [ -f docs/errors.md ]; then
  if [ -z "${TM_ERRORS_OUT:-}" ] || [ ! -f "$TM_ERRORS_OUT" ]; then
    echo "FAIL: docs/errors.md exists but TM_ERRORS_OUT does not point at build.rs output" >&2
    status=1
  elif ! cmp -s docs/errors.md "$TM_ERRORS_OUT"; then
    echo "FAIL: docs/errors.md differs from build.rs output" >&2
    diff -u docs/errors.md "$TM_ERRORS_OUT" || true
    status=1
  else
    echo "OK docs/errors.md matches build.rs output"
  fi
fi
exit $status
