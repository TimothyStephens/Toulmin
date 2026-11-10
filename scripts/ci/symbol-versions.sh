#!/usr/bin/env bash
# Item 8. Reports the glibc symbol versions a binary needs and checks them
# against a ceiling, or checks that a binary is fully static.
#
#   scripts/ci/symbol-versions.sh --static BIN
#       Fails if BIN has a program interpreter or any DT_NEEDED entry.
#   scripts/ci/symbol-versions.sh --max-glibc 2.17 BIN
#       Fails if BIN needs any GLIBC_x.y above 2.17, or any GLIBCXX / CXXABI
#       version above what CentOS 7's libstdc++ 4.8.5 provides (3.4.19 and
#       1.3.7); `--max-glibcxx` and `--max-cxxabi` override those. A fully
#       static libstdc++ needs none, which passes.
#
# Always prints: file size, interpreter, NEEDED entries, and the version
# requirements, so the CI log holds the evidence.
set -euo pipefail

mode=""
ceiling=""
glibcxx_ceiling="3.4.19"
cxxabi_ceiling="1.3.7"
while [ $# -gt 1 ]; do
  case "$1" in
    --static) mode=static; shift ;;
    --max-glibc) mode=glibc; ceiling="$2"; shift 2 ;;
    --max-glibcxx) glibcxx_ceiling="$2"; shift 2 ;;
    --max-cxxabi) cxxabi_ceiling="$2"; shift 2 ;;
    *) echo "usage: $0 (--static | --max-glibc X.Y) BIN" >&2; exit 2 ;;
  esac
done
bin="${1:-}"
if [ -z "$bin" ] || [ -z "$mode" ]; then
  echo "usage: $0 (--static | --max-glibc X.Y) BIN" >&2; exit 2
fi
if [ ! -f "$bin" ]; then
  echo "symbol-versions: $bin does not exist" >&2; exit 2
fi

echo "binary: $bin"
echo "size_bytes: $(stat -c %s "$bin")"
interp=$(readelf -lW "$bin" | sed -n 's/.*Requesting program interpreter: \(.*\)\]/\1/p' || true)
echo "interpreter: ${interp:-none}"
needed=$(readelf -dW "$bin" | sed -n 's/.*(NEEDED).*\[\(.*\)\]/\1/p' | tr '\n' ' ' || true)
echo "needed: ${needed:-none}"
# Version requirements, one per line: "GLIBC_2.17", "GLIBCXX_3.4.21", ...
versions=$(readelf -VW "$bin" | sed -n 's/.*Name: \([A-Za-z0-9_.]*\)\s*Flags:.*/\1/p' | sort -u | tr '\n' ' ' || true)
echo "version_needs: ${versions:-none}"

case "$mode" in
  static)
    if [ -n "$interp" ] || [ -n "$needed" ]; then
      echo "FAIL: binary is not static" >&2; exit 1
    fi
    echo "OK: static"
    ;;
  glibc)
    status=0
    check_ceiling() { # prefix ceiling
      local prefix="$1" limit="$2" max highest
      max=$(readelf -VW "$bin" | sed -n "s/.*Name: ${prefix}_\([0-9.]*\)\s*Flags:.*/\1/p" | sort -t. -k1,1n -k2,2n -k3,3n | tail -1)
      echo "max_${prefix,,}: ${max:-none} (ceiling $limit)"
      if [ -n "$max" ]; then
        highest=$(printf '%s\n%s\n' "$max" "$limit" | sort -t. -k1,1n -k2,2n -k3,3n | tail -1)
        if [ "$highest" != "$limit" ]; then
          echo "FAIL: needs ${prefix}_$max, ceiling is $limit" >&2; status=1
        fi
      fi
    }
    check_ceiling GLIBC "$ceiling"
    check_ceiling GLIBCXX "$glibcxx_ceiling"
    check_ceiling CXXABI "$cxxabi_ceiling"
    [ $status -eq 0 ] && echo "OK: glibc $ceiling, GLIBCXX $glibcxx_ceiling, CXXABI $cxxabi_ceiling ceilings respected"
    exit $status
    ;;
esac
