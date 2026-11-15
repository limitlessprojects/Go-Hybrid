#!/usr/bin/env bash
set -euo pipefail

echo "=== Running #3674 Falsifier F1 (Attribution Fidelity & Nil-File Safety) Suite ==="
go test -v -count=1 ./...

echo ""
echo "=== Running #3674 Falsifier F2 (Latency Benchmark < 1.2ms) ==="
go test -bench=BenchmarkWarmCheckLatency -benchtime=2s -benchmem ./...
