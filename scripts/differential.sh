#!/usr/bin/env bash
# Item 89 and the Verification Plan's Differential row: the Rust Checker
# against the Python reference checker on N generated cases.
#
#   scripts/differential.sh N [SEED]
#
# Builds examples/gencases.rs and examples/refcheck.rs (tm-checker),
# generates N cases into a temporary directory, runs both refcheck
# implementations on every case, compares the seven fields of the two
# objects (verdict, steps, columns, final_rows, final_rows_hash,
# key_map_hash, error; a side that exits non-zero or prints no JSON object
# is a difference in a field named `run`), prints the differences as a
# table (case, seed, field, Rust value, Python value), and exits 0 only when
# the table is empty. Every differing case is kept, with both outputs and
# both standard errors beside it, under target/differential/<SEED>/<case>/.
#
# SEED defaults to the current Unix time and is printed first; the same N
# and SEED reproduce the same cases (gencases is seeded per case, so
# `gencases --seed SEED --start I --n 1` regenerates one case alone).
#
# Both implementations share one contract: one case directory as the only
# argument, the item 92 expected.json object on standard output, exit 0
# whatever the verdict, non-zero only when the case cannot be run at all.
#
# Environment:
#   REFCHECK_PY  the Python side's command (default:
#                `python3 -I reference/checker/refcheck.py`); a plumbing
#                test can point it at a stub
#   JOBS         cases run at a time (default: nproc)
#   KEEP_DIR     where differing cases go (default: target/differential/SEED)
set -euo pipefail
cd "$(dirname "$0")/.."

n="${1:?usage: scripts/differential.sh N [SEED]}"
seed="${2:-$(date +%s)}"
case "$n$seed" in *[!0-9]*) echo "differential: N and SEED are non-negative integers" >&2; exit 2;; esac
py="${REFCHECK_PY:-}"
if [ -z "$py" ]; then
  if [ ! -f reference/checker/refcheck.py ]; then
    echo "differential: reference/checker/refcheck.py does not exist (item 87, P5); set REFCHECK_PY to another command" >&2
    exit 1
  fi
  py="python3 -I reference/checker/refcheck.py"
fi
jobs="${JOBS:-$(nproc 2>/dev/null || echo 2)}"
keep="${KEEP_DIR:-target/differential/$seed}"

echo "differential: $n cases, seed $seed, python side: $py"
cargo build --locked -p tm-checker --example refcheck --example gencases
rust="$PWD/target/debug/examples/refcheck"
gen="$PWD/target/debug/examples/gencases"

work=$(mktemp -d "${TMPDIR:-/tmp}/tm-differential.XXXXXX")
trap 'rm -rf "$work"' EXIT
"$gen" --out "$work/cases" --n "$n" --seed "$seed"
mkdir -p "$work/out"

# One case: both sides, outputs and exit codes under out/<case>/.
run_one() {
  local dir="$1" name out rc
  name=$(basename "$dir")
  out="$DIFF_OUT/$name"
  mkdir -p "$out"
  rc=0; "$DIFF_RUST" "$dir" > "$out/rust.json" 2> "$out/rust.stderr" || rc=$?
  echo "$rc" > "$out/rust.exit"
  # shellcheck disable=SC2086
  rc=0; $DIFF_PY "$dir" > "$out/python.json" 2> "$out/python.stderr" || rc=$?
  echo "$rc" > "$out/python.exit"
}
export -f run_one
export DIFF_OUT="$work/out" DIFF_RUST="$rust" DIFF_PY="$py"
started=$SECONDS
find "$work/cases" -mindepth 1 -maxdepth 1 -type d -print0 | sort -z \
  | xargs -0 -P "$jobs" -I{} bash -c 'run_one "$1"' _ {}
echo "differential: both sides ran on $n cases in $((SECONDS - started)) s"

# Compare, print the table, keep the differing cases.
python3 -I - "$work/cases" "$work/out" "$seed" "$keep" <<'PY'
import json, os, shutil, sys

cases_dir, out_dir, run_seed, keep = sys.argv[1:5]
FIELDS = ["verdict", "steps", "columns", "final_rows", "final_rows_hash", "key_map_hash", "error"]


def load(path):
    """The JSON object a side printed, or an error string."""
    base = path[:-5]
    with open(base + ".exit", encoding="utf-8") as f:
        rc = f.read().strip()
    with open(base + ".stderr", encoding="utf-8", errors="replace") as f:
        err = f.read().strip().splitlines()
    first = err[0] if err else ""
    if rc != "0":
        return None, f"exit {rc}: {first}"[:200]
    try:
        with open(path, encoding="utf-8") as f:
            obj = json.load(f)
    except (OSError, ValueError) as e:
        return None, f"no JSON object: {e}"[:200]
    if not isinstance(obj, dict):
        return None, "no JSON object: printed a " + type(obj).__name__
    return obj, None


def short(v, width=72):
    s = v if isinstance(v, str) and v.startswith(("exit ", "no JSON", "ok", "absent")) else json.dumps(v, ensure_ascii=False, separators=(",", ":"))
    return s if len(s) <= width else s[: width - 3] + "..."


rows = []
differing = []
names = sorted(d for d in os.listdir(cases_dir) if os.path.isdir(os.path.join(cases_dir, d)))
for name in names:
    seed = run_seed
    try:
        with open(os.path.join(cases_dir, name, "gen.json"), encoding="utf-8") as f:
            seed = str(json.load(f).get("seed", run_seed))
    except (OSError, ValueError):
        pass
    o = os.path.join(out_dir, name)
    rust, rust_err = load(os.path.join(o, "rust.json"))
    py, py_err = load(os.path.join(o, "python.json"))
    diffs = []
    if rust_err or py_err:
        diffs.append(("run", rust_err or "ok", py_err or "ok"))
    else:
        keys = FIELDS + sorted((set(rust) | set(py)) - set(FIELDS))
        for k in keys:
            a = rust.get(k, "absent")
            b = py.get(k, "absent")
            if a != b:
                diffs.append((k, a, b))
    for field, a, b in diffs:
        rows.append((name, seed, field, short(a), short(b)))
    if diffs:
        differing.append(name)

if rows:
    widths = [max(len(r[i]) for r in rows + [("case", "seed", "field", "rust", "python")]) for i in range(5)]
    def line(r):
        return "| " + " | ".join(c.ljust(w) for c, w in zip(r, widths)) + " |"
    print(line(("case", "seed", "field", "rust", "python")))
    print("| " + " | ".join("-" * w for w in widths) + " |")
    for r in rows:
        print(line(r))
    os.makedirs(keep, exist_ok=True)
    for name in differing:
        dst = os.path.join(keep, name)
        shutil.rmtree(dst, ignore_errors=True)
        shutil.copytree(os.path.join(cases_dir, name), dst)
        for f in os.listdir(os.path.join(out_dir, name)):
            shutil.copy2(os.path.join(out_dir, name, f), dst)
    print(f"differential: {len(differing)} of {len(names)} cases differ in {len(rows)} fields (seed {run_seed}); kept under {keep}/")
    sys.exit(1)
print(f"differential: no difference in any field over {len(names)} cases (seed {run_seed})")
PY
