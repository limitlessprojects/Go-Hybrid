#!/usr/bin/env bash
set -euo pipefail

echo "=== Running W-004 Positive Witness Tests ==="
go test -v ./...

echo ""
echo "=== Running W-004 Zero-Cost Benchmark ==="
go test -bench=. -benchmem ./...
