#!/bin/sh
# One command: build the compiler, emit Go for every ACCEPT corpus program,
# enumerate + certify, replay every certificate, and cross-check against the
# real Go race detector. Every number in RESULTS.md comes from this script.
set -eu

HERE=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$HERE/../.." && pwd)
WORK=${WORK:-"$HERE/.work"}
CORPUS="$REPO/tests/checker-corpus"
TARGET=${CARGO_TARGET_DIR:-"$HERE/.cargo-target"}
RUNS=${RUNS:-20}

mkdir -p "$WORK/gosrc" "$WORK/bin" "$WORK/certs"
rm -f "$WORK"/*.json

echo "== building hyb =="
RUSTV=$(rustc --version | awk '{print $2}')
GOV=$(go version | awk '{print $3}')
( cd "$REPO/compiler" && HYB_PINNED_RUST="$RUSTV" HYB_PINNED_GO="$GOV" \
    CARGO_TARGET_DIR="$TARGET" cargo build -p hyb --quiet )
HYB="$TARGET/debug/hyb"

echo "== emitting Go for every ACCEPT program (race-enabled binaries) =="
: > "$WORK/build_failed.txt"
for f in "$CORPUS"/*.hy; do
  v=$(grep -m1 'EXPECTED-VERDICT' "$f" | sed 's/.*EXPECTED-VERDICT: *//')
  case "$v" in
    ACCEPT*|ACCEPT-MANAGED*)
      stem=$(basename "$f" .hy)
      if "$HYB" build "$f" -o "$WORK/bin/$stem.bin" --race >/dev/null 2>"$WORK/bin/$stem.err"; then
        cp "$WORK/bin/$stem.bin.go" "$WORK/gosrc/$stem.go"
      else
        echo "$stem" >> "$WORK/build_failed.txt"
      fi
      ;;
  esac
done

echo "== building hcert =="
( cd "$HERE" && go build -o "$WORK/hcert" ./cmd/hcert && go build -o "$WORK/oracle" ./cmd/oracle )

echo "== go test (tamper suite + F1) =="
( cd "$HERE" && go test ./... )

echo "== corpus, pre-registered Tier B (sum chan depth <= 8) =="
"$WORK/hcert" corpus --corpus "$CORPUS" --go "$WORK/gosrc" \
  --out "$WORK/results.json" --certs "$WORK/certs"

echo "== corpus, extended bound (sum chan depth <= 16; reported separately) =="
"$WORK/hcert" corpus --corpus "$CORPUS" --go "$WORK/gosrc" \
  --out "$WORK/results-extended.json" --certs "$WORK/certs-ext" --extended

echo "== runtime oracle: real Go, -race, $RUNS runs x GOMAXPROCS {1,4,8} =="
"$WORK/oracle" -results "$WORK/results-extended.json" -bin "$WORK/bin" \
  -runs "$RUNS" -out "$WORK/oracle.json"

echo
echo "== summary =="
python3 "$HERE/tools/summary.py" "$WORK"
