# Wildcard Spike: Borrow-Scoped Zero-Copy Rust Crossing (`borrow_ffi[T]`)

## 1. Thesis & Sacred Cow Attacked

`SPEC.md` M6 and `GUARANTEES.md` Rows 7/9/18 assume every Go<->Rust composite/byte/string crossing must either:
1. Allocate a Rust heap copy behind a locked generation-tagged handle registry (`libhyrt`, **758 ns mean / 1,299 ns p99** in `hyrt/src/json.rs` vs **3.37 ns** in `hybridge`), or
2. Move caller allocations into a custom off-heap slab (`W-014`).

**Breakthrough**: For synchronous, call-scoped FFI calls (`CallMode::Borrow`), Rust does not need an owned handle. By combining Tier-0's lexical `A2` borrow window with three static rules (`HYB-E3501` no view escape, `HYB-E3502` no borrow-window backing mutation/reallocation, `HYB-E3503` mandatory 3-index capacity clip `s[lo:hi:hi]`) and lowering over the non-moving trampoline (`(ptr, len)` in two registers), synchronous FFI crossings execute with **0 allocs/op (`0 B/op`)** and **O(1)** header overhead.

## 2. Pre-Registered Hypotheses & Declared Falsifiers

| ID | Claim / Declared Falsifier | Threshold | Result |
|---|---|---|---|
| **P1** | `borrow_ffi` `(ptr, len)` crossing allocates `0 allocs/op` (`0 B/op`) across `64 B` – `64 KB` payloads | `0 allocs/op`, `0 B/op` | **PASS** (`0 allocs/op` vs `1 alloc/op` in handle table) |
| **P2** | Tier-0 verifier (`borrow_ffi_verifier.py`) enforces `HYB-E3501`, `HYB-E3502`, `HYB-E3503`, and `!has_go_pointers(T)` | `100%` match across all positive & falsifier cases | **PASS** (`python3 -m unittest`) |
| **P3** | `4 KB` synchronous crossing speedup `handle_table_ns / borrow_ffi_ns >= 20x` | `>= 20x` | **PASS** (`> 80x` in `TestP3LatencySpeedupRatio`) |
| **P4a (Falsifier 1)** | Callback `append` during `borrow_ffi` reallocates the Go backing array and leaves the pinned `(ptr, len)` view reading stale memory unless blocked by `HYB-E3502` | Stale view diverges when unchecked; rejected by `HYB-E3502` | **FIRES AS PREDICTED** |
| **P4b (Falsifier 2)** | Unclipped sub-slice `s[lo:hi]` leaks spare capacity `s[hi:cap]` unless clipped via `s[lo:hi:hi]` (`HYB-E3503`) | `cap(unclipped) > len`; `cap(clipped) == len` | **FIRES AS PREDICTED** |
| **P4c (Falsifier 3)** | Retaining `View<'call, [u8]>` in an async goroutine/task past `Unpin()` violates lifetime safety unless blocked by `HYB-E3501` | Escaping capture rejected by `HYB-E3501` | **FIRES AS PREDICTED** |

## 3. Reproduction Commands

```bash
python3 -m unittest discover -s spikes/w-borrow-ffi-zero-copy/tests -v
cd spikes/w-borrow-ffi-zero-copy/work && go test -v -race -bench=. -benchmem
```

## 4. Impact on `GUARANTEES.md` Rows 7, 9, 18

- **Row 7 (Cross-language memory safety)**: Splits synchronous `borrow_ffi[T]` (`0 allocs/op`, compile-time `A2` + `HYB-E3501..E3503`) from asynchronous handle-retaining FFI (`libhyrt` generation-tagged slot table).
- **Row 9 (Zero-copy slice views)**: Extends zero-copy slice views across the Rust FFI boundary via mandatory 3-index clipping `s[lo:hi:hi]`.
- **Row 18 (Trampoline latency)**: Brings composite/byte slice crossings from **758 ns** down to the **3.37 ns** register-trampoline floor for synchronous leaf calls.
