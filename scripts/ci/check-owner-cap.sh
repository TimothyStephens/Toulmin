#!/usr/bin/env bash
# Item 8. Two checks that the OwnerCap constructor stays inside tm-cli
# (NFR Security and privacy):
#   1. no crate except tm-owner-cap and tm-cli mentions `new_for_cli`;
#   2. every binary crate except tm-cli builds standalone, and its feature
#      graph does not enable tm-owner-cap's `construct` feature.
# In gate A the set of other binary crates is empty and the script passes.
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

# Binary crates: a crates/<name>/src/main.rs or a [[bin]] table.
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
