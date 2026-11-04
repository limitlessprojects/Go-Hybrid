#!/usr/bin/env bash
set -euo pipefail

echo "=== Running W-024 Falsifier F1 (Attribution Fidelity) Suite ==="
go test -v -run TestFalsifierF1_SpanAttributionFidelity ./...

echo ""
echo "=== Running W-024 Falsifier F2 (Latency Benchmark < 1.2ms) ==="
go test -bench=BenchmarkWarmCheckLatency -benchtime=3s -benchmem ./...
