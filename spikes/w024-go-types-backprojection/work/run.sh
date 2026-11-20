#!/usr/bin/env bash
set -euo pipefail

echo "=== Running W-024 Falsifier F1 (Span & Code Attribution Fidelity) ==="
go test -v -run TestFalsifierF1_SpanAndCodeAttribution ./...

echo ""
echo "=== Running W-024 Falsifier F2 (In-Memory Warm Check Latency < 1.2ms) ==="
go test -bench=BenchmarkWarmCheckLatency -benchtime=3s -benchmem ./...
