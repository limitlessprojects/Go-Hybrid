#!/bin/bash
# W-004 negative battery: each file must FAIL to compile when its tag is on.
set -u
cd "$(dirname "$0")"
fail=0
for f in negative/*.go; do
  if go build -tags w004neg -o /dev/null "$f" 2>/dev/null; then
    echo "NEG-FAIL: $f compiled (expected type error)"
    fail=1
  else
    echo "NEG-OK: $f rejected at compile time"
  fi
done
exit $fail
