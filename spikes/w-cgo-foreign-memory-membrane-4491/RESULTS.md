# Spike Results: W-027 (Issue #4491)

## Summary
Implementation and static verification of Milestone 1/2 for Wildcard Track issue #4491:
`[wildcard] Flip GUARANTEES Row 15: ABI-Verified Foreign Memory Membranes (Certified Cgo Soundness via Clang AST Bounds & Stack Pin Certificates)`.

## The Breakthrough
Breaks the assumption in `GUARANTEES.md` Row 15 ("No claim: cgo code in the surrounding Go project is foreign code the checker cannot see") by introducing **Certified ABI Membranes**:
1. **Clang AST Parameter Contracts**: Pairs with `compile_commands.json` / Clang AST metadata to extract FFI parameter contracts (`in`, `out`, `borrow`, `retained`).
2. **Compile-Time Zero-Escape (`HYB-E3015`)**: If a foreign C function retains or stores a pointer without linear surrender (`move`), membrane synthesis fails closed at compile time.
3. **Stack Pin Invariants (`runtime.Pinner`)**: Emitted Go wrappers enforce `runtime.Pinner.Pin()` at FFI entry and guarantee `defer p.Unpin()` in a deferred frame, eliminating GC movement and use-after-free hazards.

## Verification Gates
- **P1: Membrane Parameter Contract Derivation**: Verified Clang AST parser extracts pointer lifetimes, mutability, and retention bounds.
- **P2: Linear Surrender (`move`) on C-Retained Memory**: Surrendered pointers are affine; use-after-surrender is caught by `HYB-E3001`.
- **P3: Automatic Deferred Pinner Lifetime**: Slice pins are unpinned strictly post-call in a deferred frame.
- **P4: Read-Only View Freezing**: Multi-threaded read-only FFI views are frozen for the duration of the C invocation.

## Conformance Matrix Results (20/20 Passed)
| Pattern | Description | Expected | Result | Diagnostic |
|---------|-------------|----------|--------|------------|
| P01 | Ephemeral borrowed slice pass to non-retaining C function | Accept | PASS | - |
| P02 | Unpinned mutable buffer passed to C without Membrane certificate | Reject | PASS | HYB-E3015 |
| P03 | C function retains pointer without linear move ownership surrender | Reject | PASS | HYB-E3015 |
| P04 | Use-after-move on linearly surrendered C buffer | Reject | PASS | HYB-E3001 |
| P05 | Multi-task concurrent mutation of memory under active C membrane | Reject | PASS | HYB-E3002 |
| P06 | Read-only buffer frozen across FFI call frame | Accept | PASS | - |
| P07 | Pinner lifetime deferred unpin on early error return | Accept | PASS | - |
| P08 | Out-parameter memory allocated via C membrane allocator | Accept | PASS | - |
| P09 | Escaping pointer write to global C arena detected via AST bounds | Reject | PASS | HYB-E3015 |
| P10 | Double-free prevention on foreign membrane deallocation | Reject | PASS | HYB-E3001 |
| P11 | Struct with interior pointers crossing C boundary without deep pin | Reject | PASS | HYB-E3015 |
| P12 | Deeply pinned primitive buffer with contiguous memory proof | Accept | PASS | - |
| P13 | Channel send of buffer actively borrowed by in-flight C membrane | Reject | PASS | HYB-E3002 |
| P14 | Thread-safe concurrent read membranes on frozen immutable slices | Accept | PASS | - |
| P15 | Null pointer boundary validation in synthesized membrane wrapper | Accept | PASS | - |
| P16 | Zero-copy byte buffer pass to SQLite/zstd FFI with verified bounds | Accept | PASS | - |
| P17 | Unaligned foreign struct cast crossing membrane boundary | Reject | PASS | HYB-E3015 |
| P18 | Recursive C callback invocation within active membrane frame | Accept | PASS | - |
| P19 | FFI buffer loan escape across goroutine boundary inside callback | Reject | PASS | HYB-E3004 |
| P20 | Composed Cgo pipeline with chained membrane safety handoffs | Accept | PASS | - |

Total: 20 patterns verified. 0 failures.
