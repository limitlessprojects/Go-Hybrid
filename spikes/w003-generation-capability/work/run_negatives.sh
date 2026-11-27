#!/usr/bin/env bash
# Runs each negative control alone under the race detector and asserts it races.
# Usage: ./run_negatives.sh   (needs cgo: CC=gcc CGO_ENABLED=1)
set -u
export CGO_ENABLED=1 CC="${CC:-gcc}"
fail=0
for t in TestNeg_M0_S1_ClosureCapture TestNeg_M0_S2_MissedAlias TestNeg_M0_S3_DeferArgBeforeMove \
         TestNeg_Take_S2_MissedAlias TestNeg_Take_S3_DeferArgBeforeMove \
         TestNeg_G_S5_RawEscapeIsDeclaredLimit; do
  races=0
  for i in 1 2 3; do
    if HY_NEG="$t" go test -race -count=1 -run "^${t}\$" . 2>&1 | grep -q "WARNING: DATA RACE"; then
      races=$((races + 1))
    fi
  done
  echo "$t: DATA RACE in ${races}/3 runs"
  [ "$races" -eq 3 ] || fail=1
done
exit $fail
