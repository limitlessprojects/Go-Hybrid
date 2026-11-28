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
# Usage: bash benchmarks/workloads/run.sh [out.csv]
set -eo pipefail
csv=${1:-}
if [[ -n "$csv" && "$csv" != /* ]]; then
  csv="$PWD/$csv"
fi
cd "$(dirname "$0")"
REPO=$(cd ../.. && pwd)
export PATH="$REPO/../tools/go/bin:$HOME/.cargo/bin:$PATH"
HYB=${HYB:-$REPO/compiler/target/release/hyb}
WORK=$(mktemp -d)
pending_csv=
cleanup() {
  rm -rf "$WORK"
  if [[ -n "$pending_csv" ]]; then rm -f "$pending_csv"; fi
}
trap cleanup EXIT

# Keep the ordinary interactive output. When a report is requested, collect
# only the measured programs' stdout (not compiler or checker diagnostics).
run_twin() {
  local language="$1"
  shift
  if [[ -n "$csv" ]]; then
    "$@" | tee -a "$WORK/$language.out"
  else
    "$@"
  fi
}

echo "== Go twin =="
mkdir -p "$WORK/gowl/wl"
printf 'module hybrid.program\n\ngo 1.26\n' > "$WORK/gowl/go.mod"
cp -r "$REPO/runtime/go/"* "$WORK/gowl/"
cp go/main.go "$WORK/gowl/main.go"
(cd "$WORK/gowl" && go build -o "$WORK/wl-go" . && run_twin go "$WORK/wl-go")

echo "== Rust twin =="
(cd rust && cargo build --release --quiet && run_twin rust ./target/release/workloads)

echo "== Hybrid twins (hyb check + build; the checker runs) =="
"$HYB" check hy/load.hy
"$HYB" check hy/alloc.hy
# Keep builds outside an && list: errexit ignores a failed left-hand command
# in that list, which otherwise lets a failed build produce a green run.
"$HYB" build hy/load.hy -o "$WORK/wl-hy-load" >/dev/null
run_twin hybrid "$WORK/wl-hy-load"
"$HYB" build hy/alloc.hy -o "$WORK/wl-hy-alloc" >/dev/null
run_twin hybrid "$WORK/wl-hy-alloc"

if [[ -n "$csv" ]]; then
  if [[ -d "$csv" ]]; then
    echo "CSV output is a directory: $csv" >&2
    exit 1
  fi
  # Publish on the same filesystem so a failed or concurrent run leaves
  # readers with a complete report.
  pending_csv=$(mktemp "${csv}.XXXXXX")
  echo "lang,workload,p50_ns,p99_ns,mean_ns,n" > "$pending_csv"
  for language in go rust hybrid; do
    if ! awk -v lang="$language" '
      BEGIN { OFS = "," }
      /^(load|alloc) p50=[0-9]+ns p99=[0-9]+ns mean=[0-9]+([.][0-9]+)?ns n=[1-9][0-9]*$/ && !seen[$1]++ {
        gsub(/p50=|p99=|mean=|n=|ns/, "")
        print lang, $1, $2, $3, $4, $5
        next
      }
      { exit 1 }
      END { if (!seen["load"] || !seen["alloc"]) exit 1 }
    ' "$WORK/$language.out" >> "$pending_csv"; then
      echo "Invalid $language benchmark output: expected one load and one alloc measurement" >&2
      exit 1
    fi
  done
  mv -f "$pending_csv" "$csv"
fi
