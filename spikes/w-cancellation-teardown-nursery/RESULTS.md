# W-Spike Results — Nursery-Scoped Teardown Obligations & Drain Invariants

**Proposal:** #3773 | **Author:** `@marhabynova` | **Verifiers:** `python3 spikes/w-cancellation-teardown-nursery/teardown_verifier.py --check --live-go` & `python3 -m unittest discover -s spikes/w-cancellation-teardown-nursery/tests -p "test_*.py" -v`

## 1. Pre-Registered Hypotheses & Declared Falsifiers (`H1`–`H3`, `F1`–`F2`)

| ID | Gate | Measured Result | Verdict |
| :--- | :--- | :--- | :--- |
| **H1** | Structured nursery under 1,000 randomized early cancellation & error cycles (`work/nursery_test.go`) | 1,000 nurseries × 5 tasks = 5,000 tasks executed with random `ctx.Cancel()` timings; `diff = 0` leaked goroutines | **PASS** |
| **H2** | AST check accepts clean cancellation drain (`01`) and rejects undrained channel exit with `HYB-E3910` (`02`) and bare goroutine escapes with `HYB-E3911` (`03`) | 3/3 hermetic test cases match expected verdicts; diagnostic classification exact | **PASS** |
| **H3** | Cooperative channel drain allocation overhead (`work/nursery_test.go`) | `BenchmarkCooperativeDrain`: **371.2 ns/op**, **0 B/op**, **0 allocs/op** | **PASS** |
| **F1** | Falsifier #1: Concurrent uncoordinated send during drain causes drain queue livelock | Unfrozen upstream producer sending into draining channel causes queue livelock -> requires `HYB-E3912` (producer freeze witness) | **FIRED** |
| **F2** | Falsifier #2: Unmanaged bare `go fn()` inside nursery escapes structured lifecycle | Bare goroutine escapes lexical join -> requires `HYB-E3911` (unmanaged goroutines forbidden in checked nurseries) | **FIRED** |

## 2. Benchmark Evidence

Running `go test -v -bench=. -benchmem ./...` on AMD Ryzen (Linux x86_64):

```text
=== RUN   TestNurseryZeroLeakUnderRandomizedCancellation
--- PASS: TestNurseryZeroLeakUnderRandomizedCancellation (1.04s)
BenchmarkNurseryTeardown-12      3639248    332.4 ns/op    208 B/op    5 allocs/op
BenchmarkCooperativeDrain-12     3181958    371.2 ns/op      0 B/op    0 allocs/op
PASS
```

## 3. Cost Honesty & Compiler Integration Boundaries

- **hy-check Changes:**
  - Adds `NurseryScope` to tracking context in `walk.rs`, registering active channel endpoints as affine teardown obligations.
  - At early `?` (Expr::Try), `return`, and `ctx.done()` select arms, verifies every `rx` channel executes `.drain()` before emitting `CancelWitness.abort()`.
  - Rejects unmanaged `go` statements inside checked nurseries with `HYB-E3911`.
- **Where the Wall Is:**
  - Channels escaping into unmodeled foreign Go packages or dynamic interface values (`any`) cannot be tracked statically and must be explicitly routed through `managed("...", "interop")`.
