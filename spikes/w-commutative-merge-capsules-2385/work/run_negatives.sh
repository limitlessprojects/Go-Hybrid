#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
if go build negative/uam_after_shard_finish.go 2>/dev/null; then
  echo "FAIL: uam_after_shard_finish.go compiled"
  exit 1
fi
if go build negative/uam_after_capsule_settle.go 2>/dev/null; then
  echo "FAIL: uam_after_capsule_settle.go compiled"
  exit 1
fi
echo "PASS: both negative witness tests rejected by go build"
