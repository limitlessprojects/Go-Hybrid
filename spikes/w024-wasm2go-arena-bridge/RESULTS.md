# W-024 Spike Results — AOT Wasm-to-Go Linear-Memory Arena Bridge (#757)

## Pre-Registered Hypotheses & Declared Falsifiers

| ID | Statement | Verification Test | Status |
|---|---|---|---|
| **P1 (Own/Borrow Linear Arena)** | `OwnHandle` (`Doc`) and `BorrowHandle` (`View`) lowered to a pure-Go bounds-checked `[]byte` Wasm linear-memory arena preserve Hybrid's `serde_json` semantics with `CGO_ENABLED=0`, zero `libhyrt.a`, and zero `unsafe.Pointer`. | `TestP1_ZeroCgoLinearArena_PreservesOwnAndBorrowSemantics` | **PASS** |
| **P2 (Cross-Task Move Safety)** | Transferring `move doc` (`*Doc` owning its isolated `[]byte` linear-memory arena) across `task` and `chan` boundaries executes with zero data races under `go test -race` across 16 concurrent producers and 3,200 handoffs. | `TestP2_CrossTaskMoveTransfer_ZeroDataRaceUnderRaceDetector` | **PASS** |
| **P3 (M2 Deep-Nesting & Full Export Surface)** | All `DEEP`-classified recursive boundary operations in `interfaces/descriptors.md` (`serde_json` 128-level recursion across `Parse`, `GetArray`, `ArrayGet`, `Set`, `ToStringPretty`, `Release`, plus `regex`, `sha3`, `flate2`, `ed25519`, and `hyrt` ABI probes) execute on the growable Go goroutine stack under `CGO_ENABLED=0` without the 1 MiB C stack pool (`CrossDeep*`). | `TestP3_DeepNesting128AndFullExportSurface_ZeroCgo` | **PASS** |
| **F1 (Use-After-Free & Stale Generation Falsifier)** | **Declared Falsifier:** Any attempt to read through a `BorrowHandle` (`View`) after `doc.Free()` (`defer doc.free()` at end of `managed` scope) or after the pooled 64 KiB page is recycled under a newer generation must fail deterministically (`HYB-E3105` / `HYB-E3102`) rather than reading stale or uninitialized memory. | `TestF1_Falsifier_UseAfterFreeAndStaleGenerationIntercepted` | **FIRED AS PREDICTED** |
| **F2 (Handle-Table Slot Reuse & Short-Buffer Canary Falsifier)** | **Declared Falsifier:** Reusing a freed slot in `HandleTable` increments the 32-bit generation counter monotonically (eliminating the constant `gen = 1u32` ABA hazard in `runtime/rust/hyrt/src/lib.rs:146`), and `SerializeHandle` preserves the exact `0xa5` sentinel-canary contract (`json_text_tests` in `lib.rs:487-573`) across invalid handles, `TRY_AGAIN (3)` short buffers, and slice window writes. | `TestF2_Falsifier_HandleTableSlotReuseAndShortBufferCanary` | **FIRED AS PREDICTED** |

## Benchmark & Gate Comparison (`CGO_ENABLED=0` Pure Go Arena vs 3-Tier `cgo` / `Cross*` Bridge)

| Metric / Operation | Legacy 3-Tier `cgo` / `libhyrt.a` Bridge | W-024 `wasm2go` Linear-Memory Arena (`CGO_ENABLED=0`) | Gate / Delta |
|---|---|---|---|
| Small boundary call (`HyNop`) | `3.37 – 3.44 ns/op` (`Cross1` trampoline) / `~65–120 ns/op` (`runtime.cgocall`) | **`0.75 ns/op`** (`0 B/op`, `0 allocs/op`, inlinable Go call) | **PASS (`<= 15 ns` gate; ~4.5x faster than `Cross1`)** |
| Zero-copy `View.GetBytesZeroCopy` lookup | `~85–160 ns/op` (`Cross*` + pinned-borrow crossing) | **`62.9 ns/op`** (`0 B/op`, `0 allocs/op`) | **PASS (0 heap allocs, 0 `cgo` transitions)** |
| Pooled `Doc` parse + lookup + `Free()` | C heap `malloc`/`free` + global `Mutex<Table>` lock (`758 ns` wall) | **`383.2 ns/op`** (`64 B/op`, `1 alloc/op` header over pooled 64 KiB page) | **PASS (~2x faster than 758 ns handle-table wall)** |
| `DEEP`-class stack safety (`serde_json` depth 128, `regex` 22,976 B frame) | Overflows 4,096 B `DEFAULT` guard; requires 1 MiB off-goroutine stack pool (`CrossDeep*`) | Standard growable Go goroutine stack (`0` stack-split hazard) | **Eliminates SMALL / DEFAULT / DEEP 3-tier split** |
| `CGO_ENABLED=0` cross-compilation & `go test -race` visibility | Opaque `libhyrt.a` C heap invisible to Go race detector | 100% native Go slice accesses visible to `go test -race` | **PASS** |

## Reproduction

```bash
cd spikes/w024-wasm2go-arena-bridge/work
CGO_ENABLED=0 go test -v -count=1 ./...
CGO_ENABLED=0 go test -bench=Benchmark -benchmem ./...
```
