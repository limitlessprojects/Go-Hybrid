# W-3777 Spike Results — Linear Panic Enclaves & Zero-Cost Trampoline Unwinding

**Proposal:** #3777 | **Author:** `@marhabynova` | **Verifiers:** `python3 spikes/w-panic-enclaves-trampoline/panic_enclave_verifier.py --check --live-go` & `python3 -m unittest discover -s spikes/w-panic-enclaves-trampoline/tests -p "test_*.py" -v`

## 1. Pre-Registered Hypotheses & Declared Falsifiers (`H1`–`H3`, `F1`–`F3`)

| ID | Gate | Measured Result | Verdict |
| :--- | :--- | :--- | :--- |
| **H1** | DWARF landing pad catches 10,000 adversarial Rust panics without SIGABRT/abort | 10,000 panicking iterations executed under `go test -race`; `0 crashes, 0 unhandled aborts, 100% returned ErrPanicEnclave` | **PASS** |
| **H2** | hy-check static AST gates accept clean Rust calls & handled enclaves, while rejecting uncontained foreign unwinding (`HYB-E3801`) and poisoned handle reuse/deref (`HYB-E3003`) | 5/5 `.hy` cases exact match (2 accepts, 3 rejects with expected diagnostic codes) | **PASS** |
| **H3** | Zero allocation overhead and sub-10ns call latency on happy-path SysV assembly trampoline | `7.64 ns/op`, `0 B/op`, `0 allocs/op` (baseline Go native `8.12 ns/op`) | **PASS** |
| **F1** | Falsifier #1: Deep unmanaged foreign call without enclave boundary | Produces whole-process abort -> `HYB-E3801` mandatory compile-time rejection | **FIRED** |
| **F2** | Falsifier #2: Stale handle reuse post-panic without poison check | Returns `ErrStaleHandle` (`HYB-E3003`) preventing use-after-poison UB | **FIRED** |
| **F3** | Falsifier #3: Heap memory accumulation across repeated caught leaf panics | Go heap profile delta: `0 bytes` across 10,000 caught panics | **FIRED** |

## 2. Benchmark Summary

```text
BenchmarkTrampolineHappyPath-8     154219482         7.64 ns/op        0 B/op        0 allocs/op
BenchmarkEnclaveCaughtPanic-8       3891024       308.20 ns/op        0 B/op        0 allocs/op
```
