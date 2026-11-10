#!/usr/bin/env bash
# Item 8. The Verification Plan's smoke list, run inside the CentOS 7,
# Rocky 8, and Rocky 9 containers once with the musl binary and once with
# the glibc fallback:
#   tm init, tm file register, tm table spec, tm claim add, tm view --out,
#   tm audit all exit 0 on the workspace fixture.
#
#   scripts/ci/smoke.sh TM_BINARY [FIXTURE_DIR]
#
# Until the fixture (item 82a, P3) and the tm binary (item 83, P8) exist the
# script is a no-op that passes. It must stay bash 4.2 compatible (CentOS 7).
set -euo pipefail
tm="${1:-}"
fixture="${2:-fixtures}"

if [ -z "$tm" ] || [ ! -x "$tm" ]; then
  echo "smoke: no-op, no tm binary given or built yet (P8)"
  exit 0
fi
if [ ! -d "$fixture/claims" ] || [ ! -f "$fixture/de.tsv" ]; then
  echo "smoke: no-op, fixture $fixture is not complete yet (P3)"
  exit 0
fi

echo "smoke: $tm on $(cat /etc/redhat-release 2>/dev/null || uname -a)"
"$tm" --version
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
record="$work/record"
project="$work/project"
mkdir -p "$project"
cp -r "$fixture"/. "$project"/
cd "$project"

"$tm" init --record "$record" --project "$project"
"$tm" file register de.tsv
"$tm" table spec de.tsv --spec specs/de.json
first=$(ls -d claims/*/ | head -1)
# shellcheck disable=SC2046
"$tm" claim add $(cat "$first/args.txt") --steps "$first/steps.json" --assert "$first/assert.json"
"$tm" view --out "$work/view"
"$tm" audit
echo "smoke: OK"
