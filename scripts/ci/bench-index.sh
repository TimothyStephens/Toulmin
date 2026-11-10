#!/usr/bin/env bash
# Item 8 and NFR Budgets: a full index rebuild on the one-million-event
# chain from crates/tm-core/examples/gen-chain.rs must finish in under 60 s
# on the runner class named here.
#
#   scripts/ci/bench-index.sh [EVENTS] [LIMIT_SECONDS]
#
# Runner class the budget is measured on: GitHub-hosted `ubuntu-24.04`
# (4 vCPU, x64). A laptop-class disk is the budget's reference; the runner's
# SSD is at least that.
# No-op that passes until gen-chain.rs and tm-cli exist (P7).
set -euo pipefail
cd "$(dirname "$0")/../.."
events="${1:-1000000}"
limit="${2:-60}"

if [ ! -f crates/tm-core/examples/gen-chain.rs ] || [ ! -d crates/tm-cli ]; then
  echo "bench-index: no-op, gen-chain.rs or tm-cli does not exist yet (P7)"
  exit 0
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cargo build --release --locked -p tm-core --example gen-chain
cargo build --release --locked -p tm-cli
./target/release/examples/gen-chain --events "$events" --out "$work/record"
start=$(date +%s.%N)
./target/release/tm --record "$work/record" index rebuild
end=$(date +%s.%N)
elapsed=$(python3 -c "print(round($end - $start, 2))")
echo "bench-index: $events events rebuilt in ${elapsed}s (limit ${limit}s)"
python3 -c "import sys; sys.exit(0 if $elapsed < $limit else 1)" || {
  echo "FAIL: over budget" >&2; exit 1; }
