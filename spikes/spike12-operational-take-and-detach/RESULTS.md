# Spike 12: Operational Take-and-Detach Lowering for `move` (`W-017` / #200)

**Status:** feasibility spike for `MOONSHOT.md` ledger entry `W-017` (Slot 17, Family A, #200). Hypotheses, thresholds, and falsifiers are recorded below; note that initial model and benchmark runs were developed alongside this document in the same spike branch before the first commit (recorded transparently per SPEC §14.7).
**Code:**
- `take_detach_model.rs` — dependency-free Rust model (`184` physical lines / `154` code lines above `#[cfg(test)]`, within the `<250 LOC` gate; `12` test functions including a `30`-shape executable matrix and `8` red-team regression tests `R1`–`R8`).
- `work/take_detach_test.go` & `work/edge_test.go` — executable Go 1.26 race-detector harness and microbenchmarks (`6` positive `-race` tests, `5` `HY_NEG` negative race-detector controls, `6` benchmarks).
**Question:** If `hy-emit` lowers `move <place>` from its current identity no-op (`Expr::Move { operand, .. } => self.emit_expr(operand)`) into an operational take (`__hy_take(&x)`) plus selective header detach (`__hy_detach_<Shape>`) and 3-index slice capacity clipping (`s[:len(s):len(s)]`) while keeping the shipped CGo/Rust staticlib bridge (`hyrt` `u64` generation-checked `HandleTable`) intact:
1. Does the un-aliased fast path (`__hy_take(&x)`) achieve **zero heap allocations** and sub-nanosecond inlined cost?
2. Do the historical P0 soundness bug families (`#38`/`#105`/`#120` call/method return sharing, `#75`/`#137`/`#206` closure & rebind laundering, `#98`/`#138` slice capacity sharing) become race-free under `CC=gcc CGO_ENABLED=1 go test -race` when `may_share` triggers selective detach / capacity clipping?
3. Does `__hy_take(&h)` unify cleanly with the shipped CGo/Rust `own handle` path by zeroing the Go `uint64` handle binding to `0` (`InvalidHandle` in `hyrt`)?
4. What static checks in `hy-check` remain strictly necessary (cannot be replaced by operational lowering alone)?

---

## Scope adjustment from `MOONSHOT.md` triage (`W-017`, Slot 17)

Per the maintainer's triage scope note in `MOONSHOT.md` (`4f5c003`):
- **Dropped:** deleting the CGo/Rust staticlib bridge. The shipped `u64` generation-checked `HandleTable` in `runtime/rust/src/lib.rs` and `runtime/go/hyrt/hyrt.go` works on `arm64` and `x86_64` and already demonstrates operational ownership at the Go↔Rust boundary.
- **Retained & unified:** making `move` operationally real on the Go side (`__hy_take` + selective `__hy_detach_<Shape>` + `__hy_clip_slice`), and unifying it with the Rust `own handle` path by having `move h` zero the Go-side `uint64` handle variable to `0` (the invalid sentinel in `hyrt`).

---

## Pre-registered predictions & falsifiers (SPEC §14.7)

- **P1 (Fast-path overhead):** Inlined `__hy_take(&x)` on an un-aliased `map`, `[]T`, `struct`, or `uint64` `own handle` allocates **`0 B/op` (`0 allocs/op`)** and stays within **`≤ 0.5 ns/op`** of the current no-op lowering (`t := x`). Even with `//go:noinline`, it allocates `0 B/op` and stays `< 2.0 ns/op`.
- **P2 (Slice capacity isolation):** 3-index capacity clipping (`s[:len(s):len(s)]`) on slice moves allocates **`0 B/op` (`0 allocs/op`)** at the `move` site (`< 1.5 ns/op`) and prevents `append` in a spawned task from racing on spare backing-array capacity under `go test -race`.
- **P3 (Shared-peer race elimination & negative controls):** Under `CC=gcc CGO_ENABLED=1 go test -race`, the representative P0 alias-laundering chains (`TestChain1`, `TestChain2`, `TestChain3`) pass 500 concurrent iterations with **0 data races**, whereas disabling each mechanism (`HY_NEG=TestNeg_FastPathMissedAlias`, `HY_NEG=TestNeg_NoClip`, `HY_NEG=TestNeg_DeferArgEvaluatedBeforeMove`, `HY_NEG=TestNeg_TakeWritesCapturedSlot`, `HY_NEG=TestNeg_ShallowDetachMapOfSlices`) deterministically triggers **`WARNING: DATA RACE`**.
- **P4 (Falsifier — where operational `move` is NOT enough on its own):**
  - If an alias escapes through a channel/closure/`defer` that `may_share` does not record, choosing the zero-alloc fast path (`__hy_take` without `__hy_detach`) **still races** (`TestNeg_FastPathMissedAlias`, `TestNeg_DeferArgEvaluatedBeforeMove`, and Rust test `r8`). Therefore, `W-017` does **not** eliminate the need for conservative intra-procedural `may_share` + `defer`/closure capture accounting unless every `move` unconditionally deep-clones (which is `W-005`'s scope).
  - On the Go side, reading from a zeroed (`nil`) map or slice or calling `append` on a `nil` slice succeeds silently without panicking (`TestNilAndEmptyMoves`). Operational take therefore does **not** replace the static `HYB-E3003` use-after-move check.

---

## Mechanisms measured

| ID | Mechanism | Lowered Go shape | When emitted |
|---|---|---|---|
| **M0** | Current `hy-emit` baseline | `dst := src` | Today (`Expr::Move` is a no-op) |
| **M1** | Fast-path operational take (inlined) | `dst := __hy_take(&src)` (`v := *src; *src = zero; return v`) | `may_share[src]` is empty and no sub-slice capacity is shared |
| **M1b** | Fast-path operational take (`//go:noinline`) | `dst := hyTakeNoInline(&src)` | Measures call-boundary floor when not inlined |
| **M2** | Operational take + slice capacity clip | `dst := __hy_clip_slice(__hy_take(&src))` (`s[:len(s):len(s)]`) | `src` is (or contains) a slice that may share spare capacity |
| **M3** | Selective header detach | `dst := __hy_detach_<Shape>(__hy_take(&src))` | `may_share[src]` is non-empty at `move src` |
| **M4** | Rust `own handle` sentinel take | `dst := __hy_take(&h)` (`h` becomes `0`, `InvalidHandle` in `hyrt`) | `src` is a boundary `own handle` (`uint64`) |

---

## Reproduction commands

```bash
# 1. Standalone dependency-free Rust model (12 tests including 30-shape matrix + R1..R8)
rustc --edition=2021 --test spikes/spike12-operational-take-and-detach/take_detach_model.rs -o /tmp/spike12_model && /tmp/spike12_model

# 2. Go race-detector verification (note: CC=gcc CGO_ENABLED=1 is required when go env CC=/dev/null)
cd spikes/spike12-operational-take-and-detach/work
CC=gcc CGO_ENABLED=1 go test -race -v ./...

# 3. Run any of the 5 negative controls to observe the race detector fire when a mechanism is omitted
HY_NEG=TestNeg_FastPathMissedAlias CC=gcc CGO_ENABLED=1 go test -race -run TestNeg_FastPathMissedAlias ./...
HY_NEG=TestNeg_NoClip CC=gcc CGO_ENABLED=1 go test -race -run TestNeg_NoClip ./...

# 4. Microbenchmarks (5 runs)
go test -bench=. -benchmem -count=5 ./...
```

---

## Raw benchmark results (`go1.26.4 linux/amd64`, Intel Core i5-14400F)

| Benchmark | Run 1 | Run 2 | Run 3 | Run 4 | Run 5 | Median `ns/op` | `B/op` | `allocs/op` |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| `M0_NoopMove` | 0.3494 | 0.3833 | 0.3530 | 0.3586 | 0.3915 | **0.3586 ns/op** | `0 B` | `0` |
| `M1_HyTake_FastPath_Inline` | 0.3019 | 0.2954 | 0.2766 | 0.2993 | 0.3019 | **0.2993 ns/op** | `0 B` | `0` |
| `M1b_HyTake_FastPath_NoInline` | 1.110 | 1.120 | 1.160 | 1.136 | 1.175 | **1.136 ns/op** | `0 B` | `0` |
| `M2_HyTake_ClippedSlice` | 0.7977 | 0.7371 | 0.7505 | 0.7412 | 0.7583 | **0.7505 ns/op** | `0 B` | `0` |
| `M3_HyDetach_SharedMap8` | 71.28 | 78.65 | 73.55 | 71.91 | 74.63 | **73.55 ns/op** | `256 B` | `2` |
| `M4_HyTake_RustOwnHandle` | 0.2833 | 0.2736 | 0.3299 | 0.2697 | 0.2647 | **0.2736 ns/op** | `0 B` | `0` |

**Interpretation:**
- **M1 (`0.30 ns/op`, `0 allocs/op`)** and **M4 (`0.27 ns/op`, `0 allocs/op`)** compile down to register/stack word stores identical in cost to **M0 (`0.36 ns/op`)**.
- **M2 (`0.75 ns/op`, `0 allocs/op`)** adds a 3-index slice header capacity adjustment (`cap = len`) with zero heap allocations.
- **M3 (`73.55 ns/op`, `256 B/op`, `2 allocs/op` for an 8-entry `map[string]int`)** pays allocation cost only when `may_share` is non-empty at the `move` site.

---

## 30-shape executable matrix (`take_detach_model.rs`)

All 30 shapes are executed in `tests::executable_30_case_corpus_and_p0_matrix`:

| Cases | Category | Representative corpus / issue shapes | Expected verdict / lowered op |
|---|---|---|---|
| **1–10** | Un-aliased owned moves | `1`: scalar copy (`01-hello`, `04-pipeline`); `2`: nil `map` zero value; `3`: fresh `map` (`03-worker-pool`); `4`: fresh `[]T`; `5`: `job{id, payload}` struct; `6`: struct with `map` + `[]T`; `7`: 2-level nested struct; `8`: move after earlier peer binding was already moved (`HeapId(0)` sentinel does not trigger false detach); `9`: sequential hop `caller -> mid -> callee`; `10`: composite `Res{map, own_handle}` | `Ok(ZeroAllocTake)` + post-move heap/handle disjointness |
| **11–18** | Shared-peer / P0 report chains | `11`: direct map alias (`#38`); `12`: call/method return wrapping caller's map (`#38`/`#105`/`#120`); `13`: shared slice backing array in struct (`#98`/`#138`); `14`: partial struct sharing (`shared_m` detached, `unique_m` preserved without copy); `15`: 2-level nested composite sharing inner map (`#137`/`#206`); `16`: cross-field sibling alias (`#212`); `17`: rebind chain `p2 -> h -> worker` (`#75`/`#206`); `18`: simultaneous shared map + shared slice | `Ok(SelectiveHeaderDetach)` + post-move heap disjointness |
| **19–22** | Sub-slice capacity clipping | `19`: top-level `s[:2]`; `20`: struct field holding `s[:2]`; `21`: 2-level nested struct holding `s[:2]`; `22`: composite with unshared `map` + capacity-shared `s[:2]` | `Ok(ClippedSliceTake)` (`cap_shared` cleared at every nesting depth) |
| **23–25** | CGo/Rust staticlib `own handle` | `23`: `05-json-transform` `doc` handle; `24`: `09-go-imports-hybrid` `re` handle; `25`: `10-rust-crate-import` `hasher` handle | `Ok(RustHandleSentinelTake)` (`src` handle zeroed to `0`) |
| **26–30** | Static fail-closed gates (`diagnostics.json`) | `26`: pinned foreign `sync.Mutex` inside struct; `27`: cyclic pointer graph; `28`: non-owned borrowed parameter/GC-field origin; `29`: `const` binding move & mutation (`#309` T22); `30`: active non-lexical borrow loan (`O7`) | `26`: `Err(E3001NotSendablePinned)`<br>`27`: `Err(E3005UnprovenAtBoundary)`<br>`28`: `Err(E3004NonOwnedMove)`<br>`29`: `Err(E3006ConstImmutability)`<br>`30`: `Err(E3101LoanConflict)` |

---

## Adversarial red-team findings & regression tests (`R1`–`R8` & `N1`–`E5`)

During development we red-teamed the initial model and Go harness and fixed 7 model bugs (`R1`–`R7`) plus added explicit negative controls (`N1`, `N2`, `E1`–`E3`):

1. **`R2` (`r2_zero_sentinel_is_never_a_shared_heap`) — Zero-sentinel collision (analogous to the region-ID collision caught in PR #336):**
   - *Bug in draft v1:* `__hy_take` sets moved-from map/slice fields to `HeapId(0)`. If an earlier binding `a` was already moved (`HeapId(0)`) and a second binding `s` was moved, intersecting `my_heaps` and `peer_heaps` without filtering `HeapId(0)` treated `0` as a shared heap identity.
   - *Fix:* `StorageKind::idents()` filters out `HeapId(0)` and handle `0`, and `Ident::{Heap, Handle}` keeps heap IDs and `hyrt` handles in separate namespaces.
2. **`R1`, `R3`, `R4` (`r1_double_move_of_mixed_struct_is_use_after_move`, `r3_value_copy_double_move_rejected`, `r4_nil_map_move_is_not_use_after_move`) — Moved-ness vs. storage zero state:**
   - *Bug in draft v1:* inferring whether a binding was already moved from `is_zeroed(storage)` allowed a struct with a `ValueCopy` field (or a bare `ValueCopy`) to be moved twice, while rejecting a legal move of a `nil` map (`var m map[K]V`).
   - *Fix:* `Binding.moved: bool` tracks static/operational moved-ness independently of whether the runtime value happened to start as `nil`.
3. **`R5` (`r5_nested_cap_shared_slice_is_clipped`) — Recursive slice capacity clipping:**
   - *Bug in draft v1:* top-level `GoSlice` clipped `cap_shared`, but a sub-slice nested inside a `Composite` struct transferred with `cap_shared: true`.
   - *Fix:* `StorageKind::rebuild` clips `cap_shared: false` at every nesting depth, and `LoweredMoveOp::ClippedSliceTake` is selected whenever `transferred != b.storage`.
4. **`R6`, `R7` (`r6_duplicated_own_handle_is_rejected_fail_closed`, `r7_receiver_slot_collision_rejected`) — Rust `own handle` linearity:**
   - Unlike a Go map/slice, a Rust `own handle` behind `hyrt` cannot be cloned by a Go-side header detach. If two Go bindings hold the same live handle ID (`b := a` struct copy) or a destination slot is already occupied, `operational_move` rejects fail-closed with `E3005UnprovenAtBoundary`.

---

## Honest limitations & failure modes (must be accounted for in M1/M2)

1. **`may_share` is simplified, NOT eliminated (`r8` & `TestNeg_FastPathMissedAlias`):**
   - Selective detach (`__hy_detach_<Shape>`) only runs when `hy-check` flags `may_share[x]` as non-empty at `move x`. If an alias escapes into a closure, global, or `defer` argument that `may_share` misses, theemitter would pick the fast path (`__hy_take` only) and `TestNeg_FastPathMissedAlias` proves a data race still occurs. What `W-017` eliminates is the need to prove *which path is dead after a join/rebind*: conservative intra-procedural `may_share` safely falls back to `__hy_detach_<Shape>` instead of false-accepting a race or false-rejecting valid code.
2. **Post-move reads on Go zero values are silent (`TestNilAndEmptyMoves`):**
   - In Go, reading from a `nil` map (`m["k"]`), taking `len(s)`/`len(m)`, `range` over `nil`, or calling `append(s, v)` on a zeroed slice all succeed silently without panicking (only a `nil` map write panics). Therefore, operational `__hy_take` **does not** enforce use-after-move at runtime; `hy-check`'s static `HYB-E3003` check remains mandatory.
3. **`__hy_take(&x)` writes to the source variable slot (`TestNeg_TakeWritesCapturedSlot` & `TestNeg_DeferArgEvaluatedBeforeMove`):**
   - Because `__hy_take(&x)` zeroes `x` in place, it introduces a write to `x` at the `move` site that the current no-op lowering does not have. If a prior goroutine captured `x` by variable reference, or if `defer f(x)` evaluated `x`'s header before `move x`, a race occurs (`E1`, `E2`). `hy-check` must treat prior closure variable captures and `defer` arguments of `x` as live borrows (`HYB-E3101`) or `may_share` contributors.
4. **Shallow vs. deep detach (`TestNeg_ShallowDetachMapOfSlices`):**
   - `maps.Clone(m)` detaches only the top-level map header. For nested reference shapes (`map[K][]V`, `[]map[K]V`, structs containing pointers/slices/maps), the compiler-synthesized `__hy_detach_<Shape>` helper must recursively detach each mutable reference level, and fail closed (`HYB-E3005`) on recursive/cyclic pointer types (`*Node`).
5. **Addressability of `<place>` (`&x`):**
   - `__hy_take(&x)` requires an addressable lvalue. Moving a non-addressable expression (a call return `move f()`, a map index `move m[k]`) either binds a temporary first (where fast-path take is a trivial register move) or is rejected when moving out of a shared container (`HYB-E3004`).
6. **Model scope vs. full compiler integration:**
   - This spike is a standalone model and Go runtime benchmark harness; it does not wire `__hy_take`/`__hy_detach` into `compiler/crates/hy-emit` or run against the `.hy` parser on `tests/corpus`. That integration is the M1 deliverable if `W-017` is selected to proceed.
