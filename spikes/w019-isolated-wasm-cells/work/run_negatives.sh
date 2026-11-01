#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

for f in negative/*.go; do
  if go build "$f" >/dev/null 2>&1; then
    echo "FAIL: expected compile error for $f, but build succeeded" >&2
    exit 1
  else
    echo "PASS (rejected at compile time): $f"
  fi
done
