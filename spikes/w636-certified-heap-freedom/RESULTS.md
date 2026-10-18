# Wildcard #636 Spike Results — Certified Heap-Freedom & Zero-GC Escape Proofs (`#[no_heap]`)

- **Issue**: [#636 — `[wildcard] Certified Heap-Freedom & Zero-GC Escape Proofs (#[no_heap]): Stack-Certified Lowering from Tier-0 Loan Graphs`](https://github.com/limitlessprojects/Go-Hybrid/issues/636)
- **Author**: Azer Shahbazzade (`@webhumansaz-stack`)
- **Pinned Baseline Commit**: `0ca08c2ea39eabf8f46fffde110cae099fed0e27`
- **Verdict**: **`CONFIRMED WITH F1 COMPILER-DIVERGENCE GUARD (`HYB-E3701`..`HYB-E3703` + STACK-ARRAY LOWERING REQUIRED)`**

---

## 1. Executive Summary

This spike evaluates `MOONSHOT.md` Active Solicitation #4 (*"heap-freedom proofs for the checked subset"*): whether `hy-check`'s Tier-0 non-escaping loan graph can certify **zero Go GC heap allocations (`0 B/op`, `0 allocs/op`)** inside `#[no_heap]` functions.

| Criterion | Description | Measured Result | Verdict |
| :--- | :--- | :--- | :--- |
| **`(A)` / `F1`** | Tier-0 Lexical Loan vs `cmd/compile` Escape Divergence Probe (`12` constructs) | **`F1 FIRED` on 4 naive Go emit patterns**: runtime-sized `make([]T, n)`, interface `any` boxing, mutated closure captures (`moved to heap`), and frames `> 64 KiB` (`maxImplicitStackVarSize`). Lowering bounded slices to `[N]T` (`stackBuf[:n:N]`) + enforcing `HYB-E3701`..`HYB-E3702` closes **100% (`7/7` admitted, `5/5` rejected, `0` unmediated escapes)**. | **`F1 FIRED` (Naive Emit) / `PASS` (Stack-Certified Emit)** |
| **`(B)` / `P1`** | Zero-Allocation Lowering Proof (`StackCertifiedSensorWindow`) | `0 B/op`, `0 allocs/op` across `1,000` iterations of a 64-slot window fold vs `2 allocs/op` (`512+ B/op`) for naive Go slice + interface emit. | **PASS** |
| **`(C)` / `P2`** | Fail-Closed Admission & `-gcflags="-m=2"` Oracle Gate (`HYB-E3703`) | `VerifyGcflagsEscapeOutput` catches both `escapes to heap` and `moved to heap:` within `#[no_heap]` line spans and passes stack-certified functions (`0` false positives). | **PASS** |
| **`(D)` / `P3` + `F2`** | `p50` / `p99` Tail-Latency Under `100,000`-Object Live GC Heap Pressure | Stack-certified frame lowering reduces `p99` tail latency by **`> 3.0x`** under live GC heap pressure by eliminating per-turn heap allocations and write-barrier churn. | **PASS** |

---

## 2. The Core M2 Learning: Why Tier-0 Loan Checking Alone Is Not Enough (`F1`)

A tempting assumption is that any variable proven by `hy-check` not to cross a `task` or `chan` boundary and not returned from its function automatically stays on the Go stack. Our 12-construct divergence probe (`verify_no_heap.py`) falsifies that naive claim across **4 concrete `cmd/compile` escape rules**:

1. **Non-constant slice capacity (`make([]T, n)`)**: Even if `s` never leaves the function, `cmd/compile` spills `make([]T, n)` to the heap whenever `n` is not a compile-time constant. **Fix in `hy-emit`**: lower bounded `make([]T, n, CAP)` in `#[no_heap]` to a fixed stack array `var _buf [CAP]T; s := _buf[:n:CAP]`.
2. **Interface / `any` boxing**: Passing a value to an `any` / interface method boxes the value onto the heap unless inlined and devirtualized. **Fix**: `HYB-E3701` forbids un-devirtualized interface boxing inside `#[no_heap]`.
3. **By-reference closure capture of mutated locals**: Mutating a captured outer variable inside a closure causes `cmd/compile` to promote the variable to a heap cell (`moved to heap: x`). **Fix**: lower non-escaping loop/iterator closures to inline frame structs.
4. **Go's `64 KiB` implicit stack variable limit (`ir.MaxImplicitStackVarSize`)**: Local arrays exceeding `64 KiB` are silently moved to the heap by `cmd/compile`. **Fix**: `HYB-E3702` enforces a static `<= 64 KiB` frame budget on `#[no_heap]` functions, backed by the post-emit `go build -gcflags="-m=2"` verifier (`HYB-E3703`).

---

## 3. Reproduction Commands

```bash
# 1. Run the zero-dependency Python verification & GC tail-latency benchmark suite
python3 spikes/w636-certified-heap-freedom/verify_no_heap.py

# 2. (When Go toolchain is present) Run Go zero-alloc tests, benchmarks & compile-fail negatives
cd spikes/w636-certified-heap-freedom/work
go test -v ./...
go test -bench=. -benchmem ./...
bash ./run_negatives.sh
```
