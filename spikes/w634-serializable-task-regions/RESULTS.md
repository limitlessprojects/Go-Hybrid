# Wildcard #634 Spike Results — Task Regions as Serializable Transactions

- **Issue**: [#634 — `[wildcard] Delete sendability analysis: task regions as serializable transactions`](https://github.com/limitlessprojects/Go-Hybrid/issues/634)
- **Pinned Baseline Commit**: `0ca08c2ea39eabf8f46fffde110cae099fed0e27`
- **Verdict**: **`CONDITIONAL / PARTIAL FALSIFICATION (F3 FIRED ON SMALL & DENSE GRAPHS; PASSES ON SPARSE LARGE CYCLIC GRAPHS >= 1,000 NODES)`**

---

## 1. Executive Summary

This spike evaluates whether an explicitly opted-in `tx_region` software-transactional shared-heap membrane (`TxRegion`, `TxCell[T]`, `TxMap[K,V]`, `TxSlice[T]`, and cyclic `TxNode`) can replace Tier-0 path-sensitive sendability proofs (`GUARANTEES.md` rows 1–3, 7) for pure, retry-safe task bodies.

### Pre-Registered Criteria Verdict Summary

| Criterion | Description | Measured Result | Verdict |
| :--- | :--- | :--- | :--- |
| **`(A)` / `P1`** | Two-task shared-map mutator across `10,000` adversarial schedules | `10,000 / 10,000` schedules produced strictly reference-serializable outcomes (`(45, 30, 46)` or `(35, 70, 31)`); `20,000` commits, `7,479` conflict retries, `0` lost updates. | **PASS** |
| **`(B)` / `P2` + `F1`** | Nested maps, cyclic refs (`A <-> B`), overlapping slice views (`base[1:3]`), and escape rejection | Cyclic graph invariant (`weight_A + weight_B == 300`) preserved under concurrent traversal; overlapping `SubSlice` alias conflict caught; uninstrumented escapes rejected with `HYB-E3601`. | **PASS** |
| **`(C)` / `P3` + `F2`** | Pure retry wall: reject I/O, `cgo`, callbacks, and Rust bridge handle drops | All `5` impure/bridge fixtures (`http.Get`, `os.WriteFile`, `cgo.Call`, `foreign_callback`, and Rust `drop_own`) deterministically rejected at compile/admission time (`HYB-E3602`, `HYB-E3603`). | **PASS** |
| **`(D)` / `P4`** | Randomized `2..8` task histories with aborts and explicit cancellations | `500 / 500` randomized histories (including `176` mid-flight task cancellations) linearized 100% against the single-threaded reference oracle with zero state leakage. | **PASS** |
| **`(E)` / `P5` + `F3`** | Overhead at `1`, `10`, and `1,000` nodes vs Tier-0 baseline & `W-005` clone-on-send (`<= 2.0x` target) | **Dense / small graphs (`N=1, 10`, or `N=1,000` full-write)**: `2.00x` to `51.51x` slower than `W-005` clone-on-send (**`F3 FIRED`**). **Sparse cyclic graphs (`N=1,000`, `k=2` mutated slots)**: `0.20x` of clone-on-send (**`5.0x` faster**). | **`F3 FIRED` (Dense/Small) / `PASS` (Sparse Large)** |

---

## 2. Detailed Measurements (`verify_stm_region.py`)

### 2.1 Adversarial Schedule & Linearizability Verification (`(A)`–`(D)`)

```text
=== Running Wildcard #634 Verification Suite ===
[PASS] (A) 10,000 Adversarial 2-Task Shared-Map Schedules: {'schedules': 10000, 'non_serializable': 0, 'total_commits': 20000, 'total_aborts': 7479}
[PASS] (B) Cyclic Graph + Slice Alias Mediation + Escape Rejection: {'cyclic_invariant_sum': 300, 'slice_alias_conflict_caught': True, 'escape_fixtures_rejected': 2}
[PASS] (C) Impure I/O & Rust Bridge Handle Retry Wall: {'rejected_fixtures': 5, 'codes': ['HYB-E3602', 'HYB-E3602', 'HYB-E3602', 'HYB-E3602', 'HYB-E3603']}
[PASS] (D) 2..8 Task Randomized Histories Linearizability: {'trials': 500, 'linearizable': 500, 'cancelled_tasks_isolated': 176}
```

### 2.2 Latency & Overhead Matrix at `1`, `10`, and `1,000` Nodes (`(E)`)

| Graph Size (`N`) | Tier-0 Exclusive Baseline (`p50`) | `W-005` Clone-on-Send (`p50` / `p99`) | `W-634` STM Dense (`p50` / `p99`) | Dense vs Clone Ratio | `W-634` STM Sparse (`k=2` slots, `p50` / `p99`) | Sparse vs Clone Ratio |
| :--- | ---: | ---: | ---: | ---: | ---: | ---: |
| **`1` node** | `209 ns` | `1,250 ns` / `1,459 ns` | `2,500 ns` / `3,208 ns` | **`2.00x`** (borderline) | `6,750 ns` / `9,125 ns` (100% conflict retry) | **`5.40x` (`F3 FIRED`)** |
| **`10` nodes** | `625 ns` | `1,667 ns` / `2,625 ns` | `13,542 ns` / `18,417 ns` | **`8.12x` (`F3 FIRED`)** | `4,959 ns` / `6,875 ns` | **`2.97x` (`F3 FIRED`)** |
| **`1,000` nodes** | `65,250 ns` | `24,291 ns` / `68,292 ns` | `1,251,334 ns` / `1,620,100 ns` | **`51.51x` (`F3 FIRED`)** | `4,959 ns` / `7,250 ns` | **`0.20x` (`5.0x` faster, PASS)** |

---

## 3. What We Learned (Why `F3` Fired on Small/Dense Graphs and Where STM Wins)

1. **Per-Access Read/Write Set Accounting Dominates Small & Dense Graphs (`F3 FIRED`)**:
   - Replacing static Tier-0 sendability with per-load/store MVCC version checks (`Tx.Load` / `Tx.Store`) turns every map/slice/struct access from a direct memory operation into a hash-table lookup + epoch comparison + write-buffer allocation.
   - At `N = 10` nodes (or when a task mutates a large fraction of `N = 1,000` nodes), `W-005` deep clone-on-send (`PR #412`) copies contiguous memory in one pass without per-access bookkeeping, beating `W-634` STM by **`2.97x`–`51.51x`**.
   - At `N = 1` node with 2 concurrent writers, optimistic concurrency control guarantees that the second writer always aborts and retries (`5.40x` slower than clone-on-send).
2. **The Asymptotic Crossover (`O(k)` Touched Cells vs `O(N)` Graph Clone)**:
   - When tasks share a **large cyclic object graph (`N >= 1,000` nodes)** but each task only reads/writes a small working set (`k = 2` nodes), `W-005` clone-on-send must still traverse and clone all `1,000` nodes + memoize cycle pointers (`24,291 ns` p50), whereas `W-634` `tx_region` logs only the `2` touched `TxCell` entries (`4,959 ns` p50 — **`5.0x` faster** than clone-on-send).
3. **The Static Purity & Bridge Handle Wall Is Non-Negotiable (`HYB-E3601`..`HYB-E3603`)**:
   - Optimistic retry is unsound if *any* uninstrumented Go pointer escapes (`HYB-E3601`), if I/O or foreign callbacks execute inside the retry loop (`HYB-E3602`), or if a Rust `own` handle is dropped/consumed before commit (`HYB-E3603`). Therefore, `tx_region` cannot replace Tier-0 as the default concurrency model; it is strictly viable only as an **opt-in region construct for pure, sparse mutations over large shared cyclic graphs**.

---

## 4. Reproduction Commands

```bash
# 1. Run the zero-dependency Python verification & benchmark suite (Criteria A..E)
python3 spikes/w634-serializable-task-regions/verify_stm_region.py

# 2. (When Go toolchain is present) Run Go race-instrumented unit tests, benchmarks & compile-fail negatives
cd spikes/w634-serializable-task-regions/work
go test -race -v ./...
go test -bench=. -benchmem ./...
bash ./run_negatives.sh
```
