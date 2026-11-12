# W-022 Spike: Linear Synchronization Capsules (`AtomicCapsule[T]`, `LinearPool[T]`, `MutexCapsule[T]`)

**Proposal Issue:** #544  
**Author:** `@carelessdept`  
**Target Rows in `GUARANTEES.md`:** Row 8 (`managed("...", "mutex"|"atomic")`), Row 10 (`sync.Pool`), Row 11 (`sync/atomic.Pointer[T]`)  
**Vocabulary Alignment:** Composes directly with `W-004` (`PR #507`, `Live[R]` / `Moved[R]` zero-size phantom capability witnesses).

---

## 1. Pre-Registered Hypotheses & Falsification Criteria

Committed together with the executable artifact in `spikes/w022-linear-sync-capsules/work/`:

| ID | Prediction | Falsifier / Pass Criterion | Result |
| :--- | :--- | :--- | :--- |
| **P1** (`AtomicCapsule[T]` — Row 11) | Restricting `atomic.Pointer[T]` to linear pointee handoff (`AtomicStoreMove` / `AtomicSwapMove` consuming `Live[R] -> Moved[R]`, `Take()` swapping `nil`) eliminates the PR #52 shared-pointee race with **0 B/op, 0 allocs/op** overhead. | Fails if `go test -race -count=2` detects any race across 8 workers × 200 swaps of mutable `map[string]int`, if heap allocs > 0, or if `negative/uam_after_atomic_store.go` compiles. | **PASS** (`4.89 ns/op` vs `4.21 ns/op` raw `atomic.Pointer.Swap`; `0 B/op`, `0 allocs/op`; negative rejected by `go build`) |
| **P2** (`LinearPool[T]` — Row 10) | `PoolPutMove(pool, Live[R], item) -> Moved[R]` with a mandatory reset hook recycles composite structs (`[]byte` + `map[string]int`) across 8 workers with 0 data races and **0 allocs/op** in steady state. | Fails if `go test -race -count=2` detects any race, if steady-state allocs > 0, or if `negative/uam_after_pool_put.go` compiles. | **PASS** (`22.08 ns/op` vs `20.03 ns/op` raw `sync.Pool`; `0 B/op`, `0 allocs/op`; negative rejected by `go build`) |
| **P3** (`MutexCapsule[T]` — Row 8) | Replacing `managed("...", "mutex")` string blocks with `WithLock(func(ScopedLoan[T]))` enforces mutual exclusion with **0 B/op, 0 allocs/op**. | Fails if `go test -race -count=2` reports a race across 8 workers × 250 increments on a shared `map[string]int` or if `WithLock` allocates on the heap. | **PASS** (`11.01 ns/op` vs `3.77 ns/op` raw `sync.Mutex`, `0 B/op`, `0 allocs/op`; `+7.24 ns` is the exact `defer Unlock()` cost) |
| **P4a** (**Declared Falsifier #1**: Interior Sub-Slice Alias Across Pool Epochs) | Outer witness consumption (`Live[R] -> Moved[R]`) on `PoolPutMoveUnreset` is **insufficient by itself** if the caller sliced an interior reference field (`leaked := buf.Payload[:2]`) before `PutMove` and no deep `may_share` check + field reset runs. | **Expected to FIRE:** `TestP4a_Falsifier_InteriorSliceAliasSurvivesOuterPoolPut` must demonstrate cross-epoch mutation visibility (`0xAA -> 0x55`) through `leaked`. | **FIRED AS PREDICTED** (converges with `W-004` P4 and `W-017` P4: outer witness consumption requires Hybrid's `may_share` graph + compiler-emitted reset hook at `PoolPutMove`) |
| **P4b** (**Declared Falsifier #2**: Multi-Capsule Lock-Order Cycle) | `MutexCapsule[T].WithLock` proves data-race freedom (Row 8), **not** deadlock freedom when two capsules `capA`, `capB` are nested in `AB` vs `BA` order. | **Expected to FIRE:** `TestP4b_Falsifier_NestedMutexCapsulesCanDeadlockWithoutLockOrder` must observe the AB-BA stall within its 15ms window. | **FIRED AS PREDICTED** (Row 8 flips to guaranteed data-race freedom; lock-order deadlock requires `W-009` wait-for epoch ordering) |

---

## 2. Benchmark Results (`go test -bench=. -benchmem`)

Measured on `go1.26` (`darwin/arm64`):

| Benchmark | Latency (`ns/op`) | Bytes (`B/op`) | Allocs (`allocs/op`) | Delta vs Raw Primitive |
| :--- | ---: | ---: | ---: | :--- |
| `BenchmarkAtomicRawPointerSwap` | `4.207 ns/op` | `0 B/op` | `0 allocs/op` | baseline |
| `BenchmarkAtomicCapsuleSwapMove` | `4.888 ns/op` | `0 B/op` | `0 allocs/op` | **`+0.68 ns/op` (`0 allocs`)** |
| `BenchmarkSyncPoolRaw` | `20.03 ns/op` | `0 B/op` | `0 allocs/op` | baseline |
| `BenchmarkLinearPoolPutMove` | `22.08 ns/op` | `0 B/op` | `0 allocs/op` | **`+2.05 ns/op` (includes reset hook, `0 allocs`)** |
| `BenchmarkMutexRaw` | `3.774 ns/op` | `0 B/op` | `0 allocs/op` | baseline |
| `BenchmarkMutexCapsuleWithLock` | `11.01 ns/op` | `0 B/op` | `0 allocs/op` | **`+7.24 ns/op` (`defer Unlock` overhead, `0 allocs`)** |

---

## 3. Draft M1 Verdict & Architectural Learning

- **Verdict:** **CONDITIONAL ACCEPT (High ROI on Rows 8, 10, 11).**
- **What Holds:**
  1. **Row 11 (`atomic.Pointer[T]`):** `AtomicCapsule[T]` solves the exact soundness hole that forced `PR #52` to reject `atomic.Pointer[T]` with `HYB-E3005`. By making `StoreMove` and `SwapMove` O5.2 move-consuming sites and restricting reads to `Take()` (or `Frozen[T]`), `atomic.Pointer[T]` achieves **0-allocation, race-free composite handoff** (`4.89 ns/op`).
  2. **Row 10 (`sync.Pool`):** `LinearPool[T]` turns `sync.Pool` from an uncheckable foreign hazard into a checked O5 consuming sink (`22.08 ns/op`, `0 allocs/op`).
  3. **Row 8 (`managed` synchronization categories):** `MutexCapsule[T].WithLock` replaces string-typed `managed("...", "mutex")` permits with a checked lexical borrow window (`0 allocs/op`).
- **What Falsifiers P4a & P4b Prove:**
  1. **P4a (Convergence with W-004 & W-017):** Just as `W-004` (`PR #507`) and `W-017` (`PR #370`) proved that zero-size phantom witnesses cannot erase `may_share` for closure captures, `P4a` proves that `PoolPutMove` cannot rely on outer witness consumption alone—it **must** check that all `may_share` interior slice/map aliases of the pooled struct are dead at the `PutMove` site and emit a structural field-reset hook.
  2. **P4b (Boundary with W-009):** `MutexCapsule[T]` guarantees **data-race freedom** (Tier-0's contract), while nested multi-capsule lock-order cycles remain a protocol-liveness property unless combined with `W-009`'s wait-for epoch ordering.

---

## 4. Before / After `GUARANTEES.md` Row Semantics (Rows 8, 10, 11)

Per maintainer review on PR #545, here is the exact contract delta for `GUARANTEES.md` Rows 8, 10, and 11 showing what flips to ✅ **Guaranteed** and what remains explicitly scoped out (per falsifiers `P4a` and `P4b`):

### Row 8 — `managed("reason", "category", ...)` vs. `MutexCapsule[T]`
- **Before (`GUARANTEES.md` today):**
  > `managed("reason", "category", tickets...) { ... }` regions | Explicit hatch: linted (`HYB-E8001`), payload REQUIRED (bare block rejects `HYB-E0003`; unknown category rejects `HYB-E0004` — closed set: `mutex/atomic/actor/finalizer/interop/other`)... | **No soundness claim inside** — it is a compile-time permit, not a synchronization mechanism.
- **After (`W-022` proposed row split — Row 8a `MutexCapsule[T]` + Row 8b narrowed `managed`):**
  > **Row 8a — `MutexCapsule[T].WithLock(fn(loan ScopedLoan[T]))`:** Initialized by consuming `move initial` (`O5.2`); `MutexCapsule[T]` is Class-2 sendable across tasks. Inside `WithLock`, `loan` is a non-sendable lexical borrow (`O2`/`O6`: cannot cross `task`/`<-`, be returned, or be stored into an outer binding; rejected via `HYB-E3102`). | ✅ **Guaranteed data-race freedom** on the encapsulated value across tasks without `managed`. **Out of scope (`P4b`):** nested multi-capsule AB-BA lock-order deadlock remains a liveness property unless paired with `W-009` wait-for epoch ordering.
  > **Row 8b — `managed("reason", "category")`:** Categories `"mutex"` and `"atomic"` are deprecated in favor of `MutexCapsule[T]` and `AtomicCapsule[T]`; `managed` remains for genuinely foreign escapes (`actor/finalizer/interop/other`).

### Row 10 — `sync.Pool` vs. `LinearPool[T]`
- **Before (`GUARANTEES.md` today):**
  > `sync.Pool` | Not in the curated surface today; a value of a foreign/unmodeled type at a check-site is **unproven → rejected** (fail-closed), never silently allowed | **No claim**; fail-closed rejection instead of a false one | §8.4 C8 pattern; O4.9
- **After (`W-022` proposed row text):**
  > `LinearPool[T]` (curated `sync.Pool` wrapper) | `Get() -> Own[T]` returns a fresh owned root; `PutMove(move x)` is an `O5.2` consuming sink that freezes `x` **and all `may_share` interior slice/map aliases of `x`** (`HYB-E3003` on any post-`PutMove` use) and runs a compiler-synthesized structural reset hook before returning `x` to `sync.Pool`. Raw `sync.Pool` remains fail-closed (`HYB-E3005`). | ✅ **Guaranteed no post-`Put` reuse race** within the `LinearPool[T]` surface (`22.08 ns/op`, `0 allocs/op`). **Condition (`P4a`):** requires both static `may_share` freeze at `PutMove` and structural reset of interior reference fields.

### Row 11 — `sync/atomic` (`atomic.Pointer[T]`) vs. `AtomicCapsule[T]`
- **Before (`GUARANTEES.md` today):**
  > `sync/atomic` (`atomic.Pointer[T]`, …) | **[TBD]** joins the curated surface post-MVP; when it lands, atomic-typed values become sendable-by-type (class 2). Until then: unproven → rejected | **No claim yet**; the growth path is pre-designed, not improvised | §8.4 C8
- **After (`W-022` proposed row text):**
  > `AtomicCapsule[T]` (`sync/atomic.Pointer[T]` linear wrapper) | Class-2 sendable across tasks. Pointee handoff is strictly linear: `StoreMove(move next)` and `SwapMove(move next) -> Own[T]` consume `next` (and freeze its `may_share` peers under `O5.2`, `HYB-E3003`), and `Take() -> Own[T]` atomically swaps `nil`. Shared non-consuming `Load()` is omitted for mutable `T` (closing the `PR #52` pointee race). | ✅ **Guaranteed race-free atomic pointer handoff** (`4.89 ns/op`, `0 B/op`, `0 allocs/op`).

---

## 5. Candidate Remedy Paths for Falsifier `P4a`

Falsifier `P4a` (`TestP4a_Falsifier_InteriorSliceAliasSurvivesOuterPoolPut`) showed that consuming only the outer struct witness `Live[R] -> Moved[R]` on `PoolPutMove` does not prevent an interior slice alias (`leaked := buf.Payload[:2]`) created *before* `PutMove` from sharing the backing array with the next `Get()` consumer. Two concrete candidate conditions close `P4a`:

1. **Candidate Condition R1 (Static `may_share` Freeze + Synthesized Structural Reset — Zero Runtime Architecture Change, Tested in `TestP4a_Remedy_StructuralDetachResetClosesInteriorSliceLeak`):**
   - **Checker rule (`hy-check`):** Treat `PoolPutMove(pool, move x)` identically to a task-boundary `move x` under `O5.2`: every binding in `aliases(x) ∪ may_share(x)` (including `leaked := buf.Payload[:2]`) is marked `Frozen` at the `PutMove` statement. Any read or write of `leaked` after `PutMove` in the producer task is rejected at compile time with `HYB-E3003`, and `leaked` cannot have been sent to another task prior to `PutMove` without already freezing `buf`.
   - **Emitter rule (`hy-emit`):** Synthesize a type-directed reset function for `T` invoked inside `PoolPutMove` (`clear(m)` for map fields, `clear(s); s = s[:0]` for slice fields, recursive reset for nested structs). Because `may_share` already proves no live alias of `buf.Payload` survived into another task before `PutMove`, zeroing and reslicing `buf.Payload[:0]` is safe and keeps steady-state allocation at `0 allocs/op` (`22.08 ns/op`).
2. **Candidate Condition R2 (Composition with `W-014` `HyRegion` Linear Slabs):**
   - If `PacketBuffer` and its backing slice live in a `W-014` task-owned linear slab, `PoolPutMove` transfers the entire slab epoch and bumps its generation bit in $O(1)$, invalidating any interior slice handle without field-by-field reset.
