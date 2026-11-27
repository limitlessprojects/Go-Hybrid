#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
for neg in negative/uam_write_after_seal_map.go negative/uam_write_after_seal_slice.go negative/mutate_sealed_map_projection.go; do
  if go build -o /tmp/wseal_neg_bin "$neg" 2>/dev/null; then
    echo "FAIL: $neg compiled unexpectedly"
    exit 1
  fi
done
echo "PASS: all 3 negative witness programs rejected by go build"
