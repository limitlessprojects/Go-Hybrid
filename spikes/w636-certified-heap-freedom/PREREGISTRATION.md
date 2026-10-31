# Wildcard #636 Spike Preregistration — Certified Heap-Freedom & Zero-GC Escape Proofs (`#[no_heap]`)

- **Issue**: [#636 — `[wildcard] Certified Heap-Freedom & Zero-GC Escape Proofs (#[no_heap]): Stack-Certified Lowering from Tier-0 Loan Graphs`](https://github.com/limitlessprojects/Go-Hybrid/issues/636)
- **Author**: Azer Shahbazzade (`@webhumansaz-stack`)
- **Pinned Baseline Commit**: `0ca08c2ea39eabf8f46fffde110cae099fed0e27`
- **Scope**: Strictly isolated under `spikes/w636-certified-heap-freedom/`; no guarded compiler, runtime, or ledger files modified.

---

## 1. Hypothesis Under Test

In an opted-in `#[no_heap]` function/region, `hy-check`'s Tier-0 non-escaping loan and sendability graph (`loans.rs`, `addr_loan.rs`, `sendability.rs`), combined with stack-certified `hy-emit` lowering (fixed-capacity stack backing arrays `[N]T` sliced as `[:len:N]`, value-struct frames, and monomorphized generics) and a `cmd/compile -gcflags="-m=2"` verification gate (`HYB-E3701`..`HYB-E3703`), guarantees **zero Go GC heap allocations (`0 B/op`, `0 allocs/op`)** and eliminates GC tail-latency spikes on Hybrid's hot path.

---

## 2. Pre-Registered Acceptance & Falsification Criteria (`A`–`D`)

| ID | Criterion (from Issue #636) | Pass Condition | Falsifier (`FIRED` if) |
| :--- | :--- | :--- | :--- |
| **`(A)` / `F1`** | **Tier-0 Lexical Loan vs `cmd/compile` Escape Divergence Probe** | Enumerate where Tier-0 lexical borrow safety diverges from Go's `cmd/compile` escape analysis, and prove that `HYB-E3701`..`HYB-E3703` + stack-certified lowering (`[N]T` sliced as `[:n:N]`) close every divergence class. | Tier-0 lexical loan validity alone is assumed sufficient without `hy-emit` lowering rewrites (`F1` fires on the 4 divergence classes: dynamic `make([]T, n)`, interface `any` boxing, by-reference closure capture, and frames `> 64 KiB`). |
| **`(B)` / `P1`** | **Zero-Allocation Proof Across Composite Workloads** | Stack-certified lowering achieves `0 B/op` and `0 allocs/op` across slice pipelines, nested struct frames, and inlined aggregators where naive Go emit allocates on every call. | Any admitted `#[no_heap]` lowered shape allocates `> 0 B/op` or escapes to the Go heap. |
| **`(C)` / `P2`** | **Fail-Closed Admission & `-gcflags="-m=2"` Oracle Gate** | `task` spawns, channel sends of pointers/slices, returned references, dynamic `append` exceeding static capacity `N`, and frames `> 64 KiB` are deterministically rejected with `HYB-E3701`, `HYB-E3702`, or `HYB-E3703`. | Any heap-escaping construct passes admission silently. |
| **`(D)` / `P3` + `F2`** | **Tail-Latency (`p50` / `p99`) Under Concurrent GC Heap Churn** | Under `100,000` live heap objects and concurrent allocation churn, stack-certified `#[no_heap]` lowering reduces `p99` latency by `>= 3.0x` compared to standard heap-allocating slice/interface emit. | `p99` latency improvement is `< 3.0x` (`F2`). |

---

## 3. Diagnostic Codes Introduced

- **`HYB-E3701` (`HeapEscapeInNoHeapScope`)**: Value escapes lexical stack frame (via `return &v`, `task` spawn, `chan` send, interface boxing, or dynamic `append` beyond fixed capacity `N`) inside `#[no_heap]`.
- **`HYB-E3702` (`StackFrameBudgetExceeded`)**: Static `#[no_heap]` local frame or backing array `[N]T` exceeds Go's `64 KiB` `maxImplicitStackVarSize` stack-allocation threshold (`cmd/compile` spills `> 64 KiB` locals to the heap).
- **`HYB-E3703` (`CompilerEscapeOracleRejected`)**: Post-emit `go build -gcflags="-m=2"` verification gate detected an unexpected `escapes to heap` or `moved to heap` diagnostic on a `#[no_heap]` symbol.
