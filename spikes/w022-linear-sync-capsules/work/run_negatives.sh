#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
for f in negative/*.go; do
  if go build "$f" >/dev/null 2>&1; then
    echo "FAIL: expected $f to be rejected by go build" >&2
    exit 1
  fi
  echo "OK (rejected by go build): $f"
done
