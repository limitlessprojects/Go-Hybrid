# Wildcard #634 Spike Preregistration — Task Regions as Serializable Transactions

- **Issue**: [#634 — `[wildcard] Delete sendability analysis: task regions as serializable transactions`](https://github.com/limitlessprojects/Go-Hybrid/issues/634)
- **Pinned Baseline Commit**: `0ca08c2ea39eabf8f46fffde110cae099fed0e27`
- **Scope**: Strictly isolated under `spikes/w634-serializable-task-regions/`; no guarded compiler, runtime, or ledger files modified.

---

## 1. Hypothesis Under Test

For an explicitly opted-in `tx_region` block, Hybrid can replace Tier-0 path-sensitive alias/loan/freeze sendability proofs (`GUARANTEES.md` rows 1–3, 7) with a software-transactional shared-heap membrane (`TxRegion` + versioned `TxCell[T]`, `TxMap[K,V]`, `TxSlice[T]`, and cyclic `TxNode` handles) where:
1. Every task spawn inside `tx_region` begins an optimistic snapshot transaction (`Tx`) with a private read-set and write-set.
2. Join / region exit performs an atomic Multi-Version Concurrency Control (MVCC / TL2-style) serializable commit: validating that every read cell's version still matches the transaction's read-epoch before installing buffered writes under a monotonic commit clock.
3. Conflicts abort and deterministically retry **only** because the region body is statically restricted to retry-safe, side-effect-free pure computations (`HYB-E3601`..`HYB-E3603`).

---

## 2. Pre-Registered Acceptance & Falsification Criteria (`A`–`E`)

| ID | Criterion (from Issue #634) | Pass Condition | Falsifier (`FIRED` if) |
| :--- | :--- | :--- | :--- |
| **`(A)` / `P1`** | **Shared-Map Two-Task Mutator & 10,000 Adversarial Schedules** | Current Tier-0 rejects `01-shared-map-across-tasks.hy` (`HYB-E3001`); transactional region accepts the pure variant (`hy/01-shared-map-tx-region.hy`) and **10,000 / 10,000** adversarial interleaved schedules produce only reference-serializable outcomes with `0` lost updates. | Any schedule produces a torn read, lost update, non-serializable state, or data race. |
| **`(B)` / `P2` + `F1`** | **Deep Composite Mediation (Nested Maps, Cyclic Refs, Slice Aliases) & Escape Rejection** | Nested maps, cyclic node graphs (`A <-> B`), and slice views route all loads/stores through versioned cells; deliberate uninstrumented escape fixtures (`HYB-E3601`, `work/negative/raw_map_escape.go`) fail closed at compile time. | A single unmediated alias or interior pointer escapes a `TxCell` without a compile error (`F1`). |
| **`(C)` / `P3` + `F2`** | **Pure Retry Safety & Side-Effect / Rust-Handle Wall** | Network, file I/O, `cgo`, foreign callbacks, and unguarded Rust `own`-handle consumption inside `tx_region` are deterministically rejected at compile time (`HYB-E3602` / `HYB-E3603`, `work/negative/impure_io_in_tx.go`). | Retrying an aborted transaction duplicates an observable external side effect or double-drops a Rust bridge handle (`F2`). |
| **`(D)` / `P4`** | **Randomized 2–8 Task Histories Linearizability** | Randomly generated `2..8` task histories (including forced aborts, conflicts, and explicit task cancellations) 100% linearize against a single-threaded reference oracle with opacity preserved. | Any aborted/cancelled task leaks buffered writes or any committed history fails single-thread serializability. |
| **`(E)` / `P5` + `F3`** | **p50 / p99 Overhead at 1 / 10 / 1,000 Nodes vs Baseline & Clone-on-Send (`W-005`)** | Measure p50/p99 latency, memory, and allocations at `1`, `10`, and `1,000` nodes against ordinary exclusive baseline and `W-005` clone-on-send (`<= 2.0x` target). | Bounded pure graphs exceed `2.0x` equivalent `W-005` clone-isolation cost (`F3`). Per #634 track terms, if `F3` fires on dense/small graphs, publish the exact crossover wall as the M2 learning rather than hiding the failure. |

---

## 3. Diagnostic Admission Codes Introduced by This Spike

- **`HYB-E3601` (`UnmediatedTransactionalEscape`)**: Raw Go map, slice, pointer, or uninstrumented struct field crosses into or out of a `tx_region` without `TxCell`/`TxMap`/`TxSlice` mediation.
- **`HYB-E3602` (`ImpureCallInTransactionalRegion`)**: I/O, network, filesystem, `cgo`, reflection, `unsafe`, or foreign callback invoked inside a retryable `tx_region` task body without an explicit `managed("reason", "tx_io")` hatch.
- **`HYB-E3603` (`LinearHandleMutationInRetryableTx`)**: Consuming or dropping a Rust bridge `own` handle inside a retryable `tx_region` body (where abort + retry would double-drop or leak generation handles).
