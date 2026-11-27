#!/usr/bin/env bash
# Item 8. Three checks that the OwnerCap constructor stays inside tm-cli
# (NFR Security and privacy; 4.2 rule 4):
#   1. no crate except tm-owner-cap and tm-cli mentions `new_for_cli`;
#   2. no crate's manifest enables tm-owner-cap's `test-support` feature
#      (`OwnerCap::for_tests`) outside its `[dev-dependencies]` table, in a
#      dependency entry or a `[features]` list (P7 owner ruling 1);
#   3. every binary crate except tm-cli builds standalone, and its feature
#      graph does not enable tm-owner-cap's `construct` feature.
# In gate A the set of other binary crates is empty and check 3 passes.
set -euo pipefail
cd "$(dirname "$0")/../.."

if [ ! -d crates ]; then
  echo "check-owner-cap: no-op, crates/ does not exist yet (P1)"
  exit 0
fi

status=0
hits=$(grep -rn --include='*.rs' 'new_for_cli' crates \
  | grep -v '^crates/tm-owner-cap/' | grep -v '^crates/tm-cli/' || true)
if [ -n "$hits" ]; then
  echo "FAIL: new_for_cli referenced outside tm-owner-cap and tm-cli:" >&2
  echo "$hits" >&2
  status=1
else
  echo "OK: new_for_cli only in tm-owner-cap and tm-cli"
fi

# Check 2: `test-support` only under [dev-dependencies] (or a target's).
if ! python3 -I - crates/*/Cargo.toml <<'PY'; then status=1; fi
import re, sys
bad = []
for path in sys.argv[1:]:
    if path.endswith("/tm-owner-cap/Cargo.toml"):
        continue  # declares the feature; enables nothing
    table = ""
    for n, raw in enumerate(open(path, encoding="utf-8"), 1):
        line = raw.split("#", 1)[0].rstrip()
        m = re.match(r"^\s*\[+\s*([^\]]*?)\s*\]+\s*$", line)
        if m:
            table = m.group(1)
            continue
        if "test-support" not in line:
            continue
        in_dev = table == "dev-dependencies" or table.endswith(".dev-dependencies")
        names_cap = "tm-owner-cap" in line or table.endswith("dependencies.tm-owner-cap")
        if table == "features" or (names_cap and not in_dev) or (
            not in_dev and "tm-owner-cap" in table
        ):
            bad.append(f"{path}:{n}: {line.strip()}")
if bad:
    print("FAIL: tm-owner-cap/test-support enabled outside [dev-dependencies]:", file=sys.stderr)
    print("\n".join(bad), file=sys.stderr)
    sys.exit(1)
print("OK: test-support is enabled only under [dev-dependencies]")
PY

# Check 3. Binary crates: a crates/<name>/src/main.rs or a [[bin]] table.
bins=()
for manifest in crates/*/Cargo.toml; do
  dir=$(dirname "$manifest")
  name=$(basename "$dir")
  [ "$name" = tm-cli ] && continue
  if [ -f "$dir/src/main.rs" ] || grep -q '^\[\[bin\]\]' "$manifest"; then
    bins+=("$name")
  fi
done
if [ ${#bins[@]} -eq 0 ]; then
  echo "OK: no binary crate other than tm-cli exists yet (gate A)"
  exit $status
fi
for name in "${bins[@]}"; do
  echo "building $name standalone"
  cargo build -p "$name" --locked
  if cargo tree -p "$name" -e features --locked 2>/dev/null | grep -q 'tm-owner-cap feature "construct"'; then
    echo "FAIL: $name enables tm-owner-cap/construct" >&2
    status=1
  else
    echo "OK: $name does not enable tm-owner-cap/construct"
  fi
done
exit $status
