# W-032: Certified Heap-Freedom Proofs & Escape Elimination for Tier-0 Task Pipelines

**Issue:** [#997](https://github.com/limitlessprojects/Go-Hybrid/issues/997)  
**Author:** `@magomedta`  
**Ledger Slot:** `W-032` (Window 2026-11-01)  
**Milestone:** M1 Accepted Into Ledger  

---

## 1. Executive Summary & Problem Formulation

In high-throughput concurrent Go systems, task spawns and channel transfers (`task { ... }`, `ch <- val`) delegate value lifetimes to Go runtime garbage collection (`runtime.newobject`). Even when values satisfy Tier-0 sendability (`own[T]`), the standard compiler emits anonymous closure trampolines:
```go
go func() { /* captures outer pointer */ }()
```
which forces heap allocations, write barriers, and GC pause jitter.

Proposal **W-032** ([#997](https://github.com/limitlessprojects/Go-Hybrid/issues/997)) breaks the foundational assertion in `GUARANTEES.md` (page 1) and Row 1 of Known Limits:
> *"The table covers Tier-0 (sendability/ownership)... it explicitly does not make heap-freedom proofs for the checked subset."*

By introducing compile-time **Heap-Freedom Task Pipeline Proofs** (`@heapfree`):
1. **Zero-Escape Task Lowering:** `hy-emit` lowers task spawns not into heap closures, but into named value-receiver task structures (`type _task_N struct { ... }; func (_t _task_N) run() { ... }`).
2. **Channel Message Flatness:** Channel sends of affine `own[T]` values enforce by-value stack copies or bounded stack-allocated ring buffers.
3. **Compile-Time Allocation Budgeting:** Tasks declare explicit stack memory budgets (`@heapfree(budget=1024)`). If escape analysis detects dynamic interface promotion or escaping pointers, `hy-check` rejects the pipeline at compile time (`HYB-E4101` - `HYB-E4105`).
4. **Deterministic Performance:** Emitted Go compiles with `-gcflags="-m"` confirming `does not escape`, yielding **0 B/op** and **0 allocs/op** (`testing.AllocsPerRun == 0`).

---

## 2. Invariant & Pipeline Escape Matrix

| ID | Task Pipeline Boundary | Failure Mode in Plain Go | Hybrid @heapfree Invariant | Diagnostic Code |
|---|---|---|---|---|
| **P1** | Anonymous Task Closure Capture | Captured lexical variable escapes stack to heap via pointer reference. | Task payload is lowered to flat value-receiver struct `_task_N`. Pointer capture forbidden. | `HYB-E4101` |
| **P2** | Dynamic Interface Boxing | Passing task parameter to `any` or `interface{}` triggers `runtime.convT`. | Monomorphic dispatch enforced across task boundary. Interface boxing rejected. | `HYB-E4102` |
| **P3** | Dynamic Channel Buffer Allocation | `make(chan T, n)` with dynamic size allocates channel header and ring on heap. | Channel allocations in `@heapfree` pipelines must use static capacity or stack ring. | `HYB-E4103` |
| **P4** | Escaping Output Pointer | Task returns or shares reference `&payload` out of task execution scope. | Return of interior reference rejected; handoff requires value move (`own[T]`). | `HYB-E4104` |
| **P5** | Stack Allocation Budget Exceeded | Task local frame exceeds declared pipeline budget. | Compile-time frame budgeting enforces bounded memory layout without stack growth. | `HYB-E4105` |

---

## 3. Operational Lowering Model

Standard Go closure trampoline (Heap allocating):
```go
// Standard Go: allocates closure context on heap
val := Payload{ID: 42, Value: 3.14}
go func() {
    process(val) // val captured by ref -> escapes to heap
}()
```

Hybrid `@heapfree` Value-Synthesized Task (0 allocs/op):
```go
// Emitted Hybrid: zero heap allocation, 100% stack-spliced
type _task_worker struct {
    payload Payload
}

func (_t _task_worker) run() {
    process(_t.payload)
}

func dispatch(p Payload) {
    t := _task_worker{payload: p}
    t.run() // inlined or worker pool dispatch without runtime.newobject
}
```

---

## 4. Verification & Falsification Protocol

1. **Escape Analysis Gate (`heapfree_pipeline_gate.py`)**:
   - Analyzes `.hy` source programs with `@heapfree` task pipelines.
   - Detects all 5 pipeline escape hazards and enforces zero-allocation invariants.
2. **Benchmark Verification (`task_pipeline_benchmark_test.go`)**:
   - Executes 100,000 rapid task dispatches.
   - Asserts `testing.AllocsPerRun(100000, fn) == 0.0`.
   - Asserts `0 B/op`.
