# Spike Results (#3674): In-Memory go/types Back-Projection as the Row-13 Decider

**Track:** Wildcard Track ($500 pool)  
**Issue Reference:** #3674  
**Author:** @jpereyra243-byte (Settlement: `0xC175c34c256598C7e1AD5B15503cFcED7Ed67e10`)  
**Status:** Executable Spike Verified (`TestFalsifierF1_SpanAttributionFidelity`, `TestParseNilFileSafety`, `BenchmarkWarmCheckLatency` PASS)  

## Pre-Registration Commitment
Pre-registered falsifiers F1 (Attribution Fidelity & Parse-Error Safety) and F2 (In-Memory Latency Budget `< 1.2 ms`) are defined in `DESIGN.md`.

---

## 1. Falsifier F1: Span, Classification, and Wrapper Isolation

Verified via `go test -v -count=1 ./...` in `spikes/w024-go-types-backprojection/work` (`go1.27.3`, `windows/amd64`):

| Subtest | Error Shape | Back-Projected Span | Attributed Code | Verdict |
|---|---|---|---|---|
| `call-arity-mismatch` | `add(1)` (expected 2 args) | `add.hy:6:11` | `HYB-E1002` | **PASS** |
| `return-type-mismatch` | `return "not-an-int"` for `int` | `ret.hy:2:9` | `HYB-E1002` | **PASS** |
| `return-value-count` | `return 1` for `(int, int)` | `ret2.hy:4:9` | `HYB-E1002` | **PASS** |
| `unused-binding` | `x := 5` unread | `bind.hy:3:2` | `HYB-E1002` | **PASS** |
| `incomparable-equality-slice` | `a == b` (`[]int`) | `cmp.hy:2:9` | `HYB-E2006` | **PASS** |
| `negative-shift` | `1 << -1` | `shift.hy:2:14` | `HYB-E6005` | **PASS** |
| `invalid-array-len` | `var a [-1]int` | `arr.hy:2:9` | `HYB-E6004` | **PASS** |
| `redeclaration-block` | `var x int; var x string` | `redecl.hy:3:6` | `HYB-E1003` | **PASS** |
| `synthetic-wrapper-inline-isolation` | Inline `/*line wrap.hy:14:9*/` inside `__hy_tmp` closure | `wrap.hy:14:16` | `HYB-E1002` | **PASS** |
| `syntax-error-scanner-backprojection` | `var x = ;` via `scanner.ErrorList` | `syn.hy:8:10` | `HYB-E1002` | **PASS** |
| `TestParseNilFileSafety` | Missing `package` clause (`fileNode == nil`) | `broken.go:1:1` | `HYB-E1002` | **PASS** |

### Subtlety Observed During Verification
Go's line-start `//line file:line:col` sets `(line, col)` for **byte 0 of the following line**, not the error token on that line. Putting a non-1 column on a full statement line shifts subsequent columns by that offset. Using `//line file:line:1` for line-aligned statements and token-granular inline `/*line file:line:col*/` inside synthesized expressions/wrappers preserves exact 1-based coordinates without synthetic bleed. Likewise, syntax errors yield a `scanner.ErrorList` (and potentially `fileNode == nil`), so `CheckSource` extracts positions directly from `scanner.Error.Pos`.

---

## 2. Falsifier F2: In-Memory `go/types.Check` Latency Benchmark

Measured via `go test -bench=BenchmarkWarmCheckLatency -benchtime=2s -benchmem ./...` on `AMD Ryzen 7 5700X 8-Core Processor` (`windows/amd64`, `go1.27.3`) with `fmt`, `os`, `sync`, `time`, `errors` pre-warmed in `CachedImporter`:

| Benchmark Case | File Size | ns/op | ms/file | Bytes/op | Allocs/op | Status vs. `< 1.2 ms` Budget |
|---|---|---|---|---|---|---|
| `BenchmarkWarmCheckLatency/Small_30LOC-16` | ~30 LOC | `47,773 ns/op` | **0.048 ms** | `18,721 B/op` | `352 allocs/op` | **PASS** |
| `BenchmarkWarmCheckLatency/Medium_250LOC-16` | ~250 LOC | `636,751 ns/op` | **0.637 ms** | `285,740 B/op` | `5,463 allocs/op` | **PASS** |

---

## 3. Reproduction

```bash
cd spikes/w024-go-types-backprojection/work
CGO_ENABLED=0 go test -v -count=1 ./...
CGO_ENABLED=0 go test -bench=BenchmarkWarmCheckLatency -benchtime=2s -benchmem ./...
```
