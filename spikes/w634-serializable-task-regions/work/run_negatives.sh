#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
for f in negative/*.go; do
  if go build "$f" 2>/dev/null; then
    echo "FAIL: expected compile error for $f"
    exit 1
  else
    echo "PASS (rejected as expected): $f"
  fi
done
