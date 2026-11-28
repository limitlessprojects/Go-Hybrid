# W-4843 Spike Results — Monotonic Ownership-to-Value Sealing (`seal move -> sealed[T]`)

**Proposal:** #4843 | **Author:** `@huyvv533` | **Scope:** Strictly isolated inside `spikes/w-monotonic-seal-4843/` (zero guarded compiler, runtime, or ledger files touched)

**One-command verification:**
```bash
python3 spikes/w-monotonic-seal-4843/seal_verifier.py --check --live-go
python3 -m unittest discover -s spikes/w-monotonic-seal-4843/tests -p "test_*.py" -v
```

## 1. Pre-Registered Hypotheses & Declared Falsifiers (`P1`–`P3`, `F1`–`F3`, Remedies `R1`–`R3`)

| ID | Pre-Registered Gate | Verification Test / Harness | Verdict |
| :--- | :--- | :--- | :--- |
| **P1 (`D1` / `corpus #19` Twin + 16-Task Broadcast)** | `SealMap` & `SealSlice` consume `Own[..., Live] -> Own[..., Moved]` (`0 B/op`, `0 allocs/op`), broadcast `SealedMap` across 16 concurrent worker goroutines (`64,000` lookups) while sender concurrently reads `.Len()` & `.Get()`, and reject all 3 post-seal write programs in `work/negative/*.go` at `go build` time; `corpus #19` (`move agg` + sender read) stays `REJECT HYB-E3003` | `TestP1_ZeroCopyBroadcastAndSenderReadAfterSend`, `TestP1_NegativeWitnessRejectionByGoBuild`, `test_d1_corpus19_move_rejects_while_seal_broadcast_accepts` | **PASS** |
| **P2 (`O(1)` Zero-Allocation Transition vs `W-005`)** | `testing.AllocsPerRun` asserts `0.00 allocs/op` on both `SealMap` and `SealSlice` over 1,024-entry maps and 256-element slices; 10,000-node transition & 8-worker broadcast benchmarks beat `W-005` per-task deep-cloning and `sync.RWMutex` | `TestP2_ZeroAllocsOnSealTransition`, `BenchmarkSealTransition_10kNodes`, `BenchmarkBroadcast8Tasks_10kNodes` | **PASS** |
| **P3 (`GUARANTEES.md` Rows 5 & 9 Flip)** | Foreign callback registration (`RegisterSealedHandler`, modeling `http.HandleFunc`) serves 12 concurrent goroutines × 2,000 requests over `SealedMap` with zero write paths, while unsealed read-only captures reject with `HYB-E3013` | `TestP3_Row9_SealedForeignCallbackGuaranteesZeroMutation`, `test_row9_foreign_callback_readonly_capture_flip` | **PASS** |
| **F1 (Go Slice `append` Spare-Capacity Clobber)** | **Declared Falsifier:** Wrapping `make([]int, 2, 8)` without 3-index capacity clipping (`NaiveUnclippedSealSlice`) lets two derived `append(s, v)` reads overwrite `backing[2]` in place (`readerA[2]` silently clobbered from `111` to `222`). **Remedy `R1`:** `SealSlice` emits `s[:len(s):len(s)]` (`cap == len`, `0 B/op`), isolating all derived appends (`safeA[2] == 111`, `safeB[2] == 222` across 8 concurrent workers) | `TestF1_Falsifier_UnclippedSliceAppendCorruptsSharedCapacityAndRemedyR1` | **FIRED AS PREDICTED (`R1` verified)** |
| **F2 (Pre-Seal Interior `may_share` Alias)** | **Declared Falsifier:** Consuming only the outer `OwnSlice[..., Live] -> Moved` witness leaves a pre-seal sub-slice (`leaked := buf[:1]`) able to mutate `sealed.At(0)` from `42` to `7777` (4th independent convergence on the phantom-witness / `may_share` boundary after `#370`, `#507`, and `W-022` `#545`). **Remedy `R2`:** `seal move x` freezes `freeze.rs:43` `storage_component(st, x)` (`aliases U may_share`) against post-seal writes (`HYB-E3003`) | `TestF2_Falsifier_PreSealInteriorAliasMutatesSealedStorage`, `check_f2_pre_seal_may_share_write` | **FIRED AS PREDICTED (`R2` verified)** |
| **F3 (Stateful Interior Closure / `chan` / Interface Laundering)** | **Declared Falsifier:** Wrapping `struct { NextID func() int }` without recursive purity checking lets `cfg.Value().NextID()` mutate captured state across reads (`1 != 2`). **Remedy `R3`:** `is_sealable(T)` (`HYB-E3015`) rejects interior closures, channels, interfaces, foreign opaque handles, mutable pointers, and `managed`-tainted bindings | `TestF3_Falsifier_StatefulInteriorClosureLaundersMutationThroughSealedStruct`, `check_f3_unsealable_shapes` | **FIRED AS PREDICTED (`R3` verified)** |

## 2. Measured 10,000-Node Benchmark Comparison (`Go 1.26`, `CGO_ENABLED=0`)

*(Host measurement environment: `go1.26.4 windows/386`, AMD Ryzen 5 6600H; Linux `amd64` `-race` leg runs on GitHub Actions CI).*

| Benchmark (`10,000` nodes) | `seal move` (`O5.7` / Class `4b`) | `W-005` Deep-Clone (`#342`) | `sync.RWMutex` (`RLock`) | Speedup / Allocation Delta |
| :--- | :--- | :--- | :--- | :--- |
| **Map transition (`10k` entries)** | **`0.51 ns/op`** (`0 B/op`, `0 allocs/op`) | `551,847 ns/op` (`217,536 B/op`, `33 allocs/op`) | — | **`1,075,934x` faster**, `-217.5 KB/op` |
| **Slice transition (`10k` ints, `[:n:n]`)** | **`0.50 ns/op`** (`0 B/op`, `0 allocs/op`) | `8,924 ns/op` (`40,960 B/op`, `1 alloc/op`) | — | **`17,869x` faster**, `-40.9 KB/op` |
| **8-worker fan-out (`512` lookups/worker + spawn)** | **`38,316 ns/op`** (`0` clone bytes) | `3,565,312 ns/op` (`1,216,578 B/op`, `289 allocs/op`) | `143,833 ns/op` (cache-line lock bounce) | **`93.1x` faster than `W-005`**, **`3.75x` faster than `RWMutex`** |

## 3. Minimal `.hy` Today-vs-Sealed Repros (`hy/`)

- `hy/01-d1-corpus19-move-vs-seal_accept.hy` — resolves `SPEC-LANGUAGE.md` §13.2 `D1`: keeps `corpus #19` (`results <- move agg; len(agg)`) as `REJECT HYB-E3003` while admitting `let snap = seal move agg` for 2-worker zero-copy broadcast + concurrent sender `snap.len()`.
- `hy/02-row9-readonly-capture_reject.hy` — demonstrates the live `GUARANTEES.md` Row 9 hole (read-only foreign `http.handle_func` capture racing with subsequent parent-scope `routes["/admin"] = 403`) and pins `REJECT HYB-E3013` unless `seal move routes` (or `managed("...", "interop")`) is used.
- `hy/03-falsifiers-e3003-e3015_reject.hy` — pins `HYB-E3003` on post-seal writes through pre-seal `may_share` sub-slices (`R2`) and `HYB-E3015` on `seal move` of structs carrying interior closures (`R3`).

## 4. Verbatim Before / After `GUARANTEES.md` Row Semantics

| Row | Surface | Before (`main`) | After (`O5.7` Monotonic Sealing + `R1`/`R2`/`R3`) |
| :--- | :--- | :--- | :--- |
| **5** | Closure bodies | **Guaranteed** for boundary statements and captured-write foreign escapes (`HYB-E3013`); **captures without a boundary statement or an escape write remain outside** | **Guaranteed** for boundary statements and **all** foreign-callback reference captures (`HYB-E3013` requires `const` or `sealed[T]` unless `managed("...", "interop")`) |
| **6** | `move` / `seal move` / freeze | **Guaranteed**: a moved value cannot be used by the sender afterwards (`HYB-E3003`; `D1` contested on sender reads) | **Guaranteed**: `move` transfers exclusive mutable ownership (`1-to-1`, freezing reads & writes); `seal move` consumes `storage_component` (`R2`), clips slice capacity `[:n:n]` (`R1`), checks `is_sealable` (`R3`, `HYB-E3015`), and yields Class-4b `sealed[T]` (`1-to-N` broadcast + sender reads, discharging `D1`) |
| **9** | Values escaping into Go APIs (`net/http` handlers, callbacks) | **Guaranteed** (`HYB-E3013`) for captured-write escapes; **read-only captures are outside — Go's DRF-SC with `-race` as the backstop** | **Guaranteed** (`HYB-E3013`) for **both** write and read-only captures of mutable state: non-`const` reference captures must be `sealed[T]` (zero writers in `storage_component`) or take `managed("...", "interop")` |
