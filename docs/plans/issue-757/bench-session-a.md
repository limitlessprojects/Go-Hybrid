# Session A Baseline Benchmark: hy_nop Small-Call Overhead

## Benchmark Environment & Execution
- **Command**: `cd runtime/go && go test -bench=BenchmarkSmallCallNop ./hybench/...`
- **Target Gate**: ≤ 15 ns/op

## Baseline Results
```
goos: linux
goarch: amd64
pkg: hybrid.program/hybench
cpu: Intel(R) Xeon(R) Processor @ 2.30GHz
BenchmarkSmallCallNop-4   	11729390	       102.2 ns/op	       0 B/op	       0 allocs/op
PASS
ok  	hybrid.program/hybench	1.317s
```

## Observations
- Current `execWasm` interpreter path yields **102.2 ns/op**.
- AOT transpilation via `staticEmit` will eliminate interpreter stack decoding loops and `sync.Pool` lookups for `hy_nop` and its direct call tree, targeting **≤ 15 ns/op**.
