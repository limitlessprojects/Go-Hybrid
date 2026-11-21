# Spike Results: W-027 (Issue #4491)

## Summary
Executable feasibility spike artifact supporting Wildcard Proposal #4491:
`[wildcard] Flip GUARANTEES Row 15: ABI-Verified Foreign Memory Membranes (Certified Cgo Soundness via Clang AST Bounds & Stack Pin Certificates)`.

All code and verification artifacts are isolated within `spikes/w-cgo-membrane-bounds-4491/`, leaving core compiler crates completely untouched.

## Verification Gates
- **P1: Membrane Pinning Lifecycle**: In safe Hybrid, passing pointers or slices across foreign C boundaries automatically synthesizes a stack-pinned membrane wrapping `runtime.Pinner`. Pinned before FFI call, deferred unpin post-call.
- **P2: Clang AST Parameter Contract**: Categorizes foreign parameters into `borrow` (C function does not retain pointer past return), `move` (C takes ownership / frees pointer), and `retained` (C stores pointer in global or async worker; requires `managed("...", "cgo_retain")`).
- **P3: Concurrent Mutation Rejection (`HYB-E3016`)**: Passing a buffer to C locks the buffer into an exclusive `FrozenView` for the duration of the FFI frame. Attempting concurrent mutation or task transfer during the FFI frame is statically rejected.
- **P4: Unpinned Escaping Pointer Rejection (`HYB-E3015`)**: Passing an unpinned Go pointer or unverified reference to a retaining C ABI function without a membrane certificate is statically rejected.
- **P5: Zero-Allocation Overhead**: Membrane wrapping executes with `0 B/op` and `0 allocs/op` on the happy path.

## Conformance Matrix Results (20/20 Passed)
| Pattern | Description | Expected | Result | Diagnostic |
|---------|-------------|----------|--------|------------|
| P01 | Basic stack-pin membrane around C slice pass | Accept | PASS | - |
| P02 | Unpinned slice passed to retaining foreign C call | Reject | PASS | HYB-E3015 |
| P03 | Concurrent slice mutation during active C FFI frame | Reject | PASS | HYB-E3016 |
| P04 | Linear ownership surrender (`move`) to C deallocator (`free`) | Accept | PASS | - |
| P05 | Use-after-move following C ownership transfer | Reject | PASS | HYB-E3003 |
| P06 | Nested C FFI calls with independent membrane pin frames | Accept | PASS | - |
| P07 | Read-only const buffer loan to C (`const char*`) allows concurrent reads | Accept | PASS | - |
| P08 | Mutable buffer loan to C (`char*`) enforces exclusive borrow | Reject | PASS | HYB-E3016 |
| P09 | Null pointer rejection on non-nullable C parameter contract | Reject | PASS | HYB-E3015 |
| P10 | Bounded slice capacity enforcement (`len <= cap` bounds proof) | Accept | PASS | - |
| P11 | Deferred unpin executed even on panic/recovery within C wrapper | Accept | PASS | - |
| P12 | Multi-buffer pin batching in single FFI membrane frame | Accept | PASS | - |
| P13 | Retained C pointer stored in global without permit | Reject | PASS | HYB-E3015 |
| P14 | Retained C pointer with explicit `managed("...", "cgo_retain")` | Accept | PASS | - |
| P15 | Zero-allocation stack membrane overhead (`0 B/op`, `0 allocs/op`) | Accept | PASS | - |
| P16 | Zero data races under concurrent goroutine FFI invocations | Accept | PASS | - |
| P17 | Struct with embedded Go pointer passed to C without deep pin | Reject | PASS | HYB-E3015 |
| P18 | Primitive scalar pass (`int32`, `float64`) bypasses membrane overhead | Accept | PASS | - |
| P19 | C callback into Go with active membrane loan validation | Accept | PASS | - |
| P20 | 100,000 stress invocations under concurrent execution without fault | Accept | PASS | - |

## Benchmarks & Live Go Invariant
Measured in `work/`:
```text
BenchmarkDirectCgoMembrane-8    10000000   112 ns/op   0 B/op   0 allocs/op
PASS
ok  spikes/w-cgo-membrane-bounds-4491/work  1.42s
```
Zero allocations per FFI membrane call, verified under `-race` with 1,000 concurrent goroutines.
