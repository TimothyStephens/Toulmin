#!/usr/bin/env bash
# Item 8 and NFR Budgets: a full index rebuild on the one-million-event
# chain from crates/tm-record/examples/gen-chain.rs (item 40) must finish
# in under 60 s on the runner class named here. The runner is tm-core's
# examples/index-rebuild.rs (item 76), which opens the record, rebuilds the
# index into a file in a temporary directory, and prints the elapsed
# seconds (the P7 checkpoint).
#
#   scripts/ci/bench-index.sh [EVENTS] [LIMIT_SECONDS]
#
# Runner class the budget is measured on: GitHub-hosted `ubuntu-24.04`
# (4 vCPU, x64). A laptop-class disk is the budget's reference; the runner's
# SSD is at least that.
set -euo pipefail
cd "$(dirname "$0")/../.."
events="${1:-1000000}"
limit="${2:-60}"

if [ ! -f crates/tm-core/examples/index-rebuild.rs ]; then
  echo "bench-index: no-op, crates/tm-core/examples/index-rebuild.rs does not exist yet (P7)"
  exit 0
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cargo build --release --locked -p tm-record --features gen-chain --example gen-chain
cargo build --release --locked -p tm-core --example index-rebuild
./target/release/examples/gen-chain --events "$events" --out "$work/record"
start=$(date +%s.%N)
./target/release/examples/index-rebuild --record "$work/record" --index "$work/index.sqlite"
end=$(date +%s.%N)
elapsed=$(python3 -c "print(round($end - $start, 2))")
echo "bench-index: $events events rebuilt in ${elapsed}s (limit ${limit}s)"
python3 -c "import sys; sys.exit(0 if $elapsed < $limit else 1)" || {
  echo "FAIL: over budget" >&2; exit 1; }
