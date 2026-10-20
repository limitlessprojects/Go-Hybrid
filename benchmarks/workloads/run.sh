#!/usr/bin/env bash
# run.sh — build + run the three-language workload triplet (PLAN row 18).
# Shapes are shared BY CONSTRUCTION (they are emitted by shape.py); this
# script just builds each twin its own way and prints the three result
# lines so the numbers land side by side:
#
#   Go    : the runtime module pattern (hybench histogram, no cgo needed
#           for the workloads themselves — the module just carries it)
#   Rust  : cargo build --release (std only, no deps)
#   Hybrid: hyb check + hyb build (the checker runs on every workload)
#
# Usage: benchmarks/workloads/run.sh [out.csv]
set -e
cd "$(dirname "$0")"
REPO=$(cd ../.. && pwd)
export PATH="$REPO/../tools/go/bin:$HOME/.cargo/bin:$PATH"
HYB=${HYB:-$REPO/compiler/target/release/hyb}
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

echo "== Go twin =="
mkdir -p "$WORK/gowl/wl"
printf 'module hybrid.program\n\ngo 1.26\n' > "$WORK/gowl/go.mod"
cp -r "$REPO/runtime/go/"* "$WORK/gowl/"
cp go/main.go "$WORK/gowl/main.go"
(cd "$WORK/gowl" && go build -o "$WORK/wl-go" . && "$WORK/wl-go")

echo "== Rust twin =="
(cd rust && cargo build --release --quiet && ./target/release/workloads)

echo "== Hybrid twins (hyb check + build; the checker runs) =="
"$HYB" check hy/load.hy
"$HYB" check hy/alloc.hy
# Keep builds outside an && list: errexit ignores a failed left-hand command
# in that list, which otherwise lets a failed build produce a green run.
"$HYB" build hy/load.hy -o "$WORK/wl-hy-load" >/dev/null
"$WORK/wl-hy-load"
"$HYB" build hy/alloc.hy -o "$WORK/wl-hy-alloc" >/dev/null
"$WORK/wl-hy-alloc"

if [ -n "$1" ]; then
  {
    echo "lang,workload,p50_ns,p99_ns,mean_ns,n"
  } > "$1"
  echo "(csv append mode: rerun with a parser — the machine lines above carry p50/p99 per twin)"
fi
