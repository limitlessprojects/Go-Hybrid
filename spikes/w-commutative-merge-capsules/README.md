# W-Commutative-Merge-Capsules: Certified Commutative Merge Capsules Spike

Research Spike and Executable Verification Suite for Go-Hybrid Wildcard Proposal [#2385](https://github.com/limitlessprojects/Go-Hybrid/issues/2385):
*"Certified Commutative Merge Capsules (`merge[M]`): Wait-Free Parallel Accumulation Without Channels, Atomics, or managed Permits"*.

---

## 1. Executive Summary & Core Motivation

Parallel accumulation in concurrent Go typically suffers from one of three structural trade-offs:
1. **Shared Mutex / Atomics (`sync.Mutex`, `sync/atomic`)**: High cacheline bouncing and lock contention under high worker counts.
2. **Channel-Based Accumulation (`chan T`)**: Goroutine scheduling overhead, channel buffer pressure, and memory allocation per message.
3. **Managed Permits**: Complex lifetime tracking with runtime borrow checking.

**Proposal #2385** introduces **Certified Commutative Merge Capsules (`merge[M]`)**:
- Each spawned worker task receives an **affine `Shard[M]`** possessing exclusive, unshared ownership over its local accumulator.
- Worker mutations execute **completely wait-free with zero synchronization** (0 mutexes, 0 atomics, 0 channels).
- When worker tasks complete, the capsule's `Settle()` operation folds all shards back into the root accumulator via a **deterministic $O(\log N)$ pairwise tree reduction**.
- To guarantee race-freedom and determinism regardless of task completion order, the reduction combiner function is **statically certified** by `hy-check` as pure, associative, and commutative. Non-commutative reductions (such as string/slice concatenation) are rejected at compile time (`HYB-E3301`) and redirected to spawn-indexed `fold[M]`.

---

## 2. Architecture & Invariants

```
                [Parent Scope: MergeCapsule[M]]
                         |
       +-----------------+-----------------+
       | Split()         | Split()         | Split()
       v                 v                 v
  Worker 1          Worker 2          Worker N
 [Shard[M] #1]     [Shard[M] #2]     [Shard[M] #N]
 (Val mutation:    (Val mutation:    (Val mutation:
  Wait-Free Local)  Wait-Free Local)  Wait-Free Local)
       |                 |                 |
       +-----------------+-----------------+
                         v
              capsule.Settle()
                         v
        Deterministic O(log N) Pairwise Tree Fold
                         v
              Certified Root Result M
```

### Core Invariants:
1. **Affine Task Shards**: Each worker task receives a single `Shard[M]`. The parent scope cannot read or mutate the root accumulator while child shards are live.
2. **Deterministic Pairwise Tree Fold**: `treeFold` reduces $N$ shards in $\lceil \log_2 N \rceil$ stages, avoiding the serialization bottleneck of linear accumulation.
3. **Static Law-Check Gate**: The combiner `func(a *M, b M)` must satisfy:
   - **Purity**: Zero I/O, zero time queries, zero global mutations, zero panic/unwind escapes (`HYB-E3302`).
   - **Commutativity**: $A \oplus B = B \oplus A$.
   - **Associativity**: $(A \oplus B) \oplus C = A \oplus (B \oplus C)$.
4. **Early-Abort Safety**: `capsule.Abort(err)` cascades cancellation through `capsule.Context()` allowing immediate worker exit without leaking goroutines or deadlocking `Settle()`.

---

## 3. Static Decidability Gate (`verifier.py`)

The static commutativity verifier executes an AST-level symbolic evaluation with Associative-Commutative (AC) term canonicalization:
- **Symbolic State Representation**: Tracks symbolic terms for each struct field across permutations.
- **AC-Unification Engine (`SymExpr`)**: Automatically flattens and normalizes associative-commutative operators (`+`, `*`, `|`, `&`, `^`, `max`, `min`).
- **Commutativity Gate**: Verifies that combining state $B$ into state $A$ yields an identical symbolic term to combining state $A$ into state $B$.
- **Associativity Gate**: Verifies that $(A \oplus B) \oplus C \equiv A \oplus (B \oplus C)$.
- **Sub-Millisecond Verification**: Runs in **<0.6 ms** per reducer, beating the <20 ms compiler gate by >30x.

---

## 4. Falsifier & Benchmark Corpus (30/30 Verified)

The verification harness (`test_verifier.py`) tests a 30-case corpus consisting of 15 legal commutative reducers and 15 illegal reducers:

### A. Legal Reducer Corpus (15/15 Passed)
| Case ID | Reduction Operation | Result | Latency |
|:---|:---|:---:|:---:|
| `legal_add` | `a.val += b.val` (Sum Monoid) | `PASS` | 0.245ms |
| `legal_multi_field` | `a.x += b.x; a.y += b.y` | `PASS` | 0.361ms |
| `legal_mult` | `a.val *= b.val` (Product Monoid) | `PASS` | 0.212ms |
| `legal_bitor` | `a.flags \|= b.flags` (Bitwise OR Semilattice) | `PASS` | 0.188ms |
| `legal_bitand` | `a.mask &= b.mask` (Bitwise AND Semilattice) | `PASS` | 0.206ms |
| `legal_bitxor` | `a.checksum ^= b.checksum` (XOR Monoid) | `PASS` | 0.187ms |
| `legal_max_func` | `a.mx = max(a.mx, b.mx)` (Max Semilattice) | `PASS` | 0.250ms |
| `legal_min_func` | `a.mn = min(a.mn, b.mn)` (Min Semilattice) | `PASS` | 0.237ms |
| `legal_cond_max` | `if b.val > a.val: a.val = b.val` | `PASS` | 0.287ms |
| `legal_telemetry` | Multi-metric aggregation (Hits, Bytes, MaxLatency) | `PASS` | 0.504ms |
| `legal_stats4` | 4-field composite statistical accumulator | `PASS` | 0.568ms |
| `legal_bin_sum` | Binary tree addition `a.sum = a.sum + b.sum` | `PASS` | 0.233ms |
| `legal_bit_and_add` | Dual-field integer addition & mask reduction | `PASS` | 0.328ms |
| `legal_nested` | Nested expression tree folding | `PASS` | 0.321ms |
| `legal_max_add` | Max latency with total operation counter | `PASS` | 0.369ms |

### B. Illegal Reducer Corpus (15/15 Rejected)
| Case ID | Attempted Mutation | Code | Rejection Cause |
|:---|:---|:---:|:---|
| `illegal_sub` | Subtraction (`a.val -= b.val`) | `HYB-E3301` | Non-commutative operator `Sub` |
| `illegal_div` | Division (`a.val /= b.val`) | `HYB-E3301` | Non-commutative operator `Div` |
| `illegal_asym_scale` | Asymmetric scale (`a.val = a.val*2 + b.val`) | `HYB-E3301` | Commutativity violation |
| `illegal_overwrite` | Last-write-wins (`a.last = b.last`) | `HYB-E3301` | Non-commutative overwrite |
| `illegal_print` | Console I/O (`print(...)`) | `HYB-E3302` | Impure side-effect |
| `illegal_io` | System write (`os.write(...)`) | `HYB-E3302` | Impure side-effect |
| `illegal_panic` | Panic escape (`panic(...)`) | `HYB-E3302` | Impure control flow escape |
| `illegal_time` | Wall-clock query (`time.now()`) | `HYB-E3302` | Non-deterministic state |
| `illegal_mod` | Modulo (`a.val %= b.val`) | `HYB-E3301` | Non-commutative operator `Mod` |
| `illegal_concat` | String concat (`a.text = a.text + b.text`) | `HYB-E3301` | Non-commutative; redirect to `fold[M]` |
| `illegal_cond_asym` | Asymmetric conditional update | `HYB-E3301` | Order-dependent state assignment |
| `illegal_rev_sub` | Reverse subtraction (`b.val - a.val`) | `HYB-E3301` | Non-commutative binary operator |
| `illegal_asym_mult` | Asymmetric operand scaling (`a.val + b.val*3`) | `HYB-E3301` | Commutativity violation |
| `illegal_shift` | Bit shift (`a.val << b.val`) | `HYB-E3301` | Non-commutative operator |
| `illegal_external_call` | Network call (`send_network(b)`) | `HYB-E3302` | Impure external side-effect |

---

## 5. Concurrency & Contention Scaling

`merge_test.go` exercises `MergeCapsule[M]` across 64 parallel workers:
1. **Race Freedom**: Full test suite passes clean under `-race` with 64 concurrent goroutines performing $6.4 \times 10^5$ accumulator updates.
2. **Determinism**: 10 repeated randomized runs with asymmetric per-worker loads yield identical bitwise results.
3. **Scaling Benchmark**:
   - `MergeCapsule`: Linear scale, wait-free per worker ($0$ mutex overhead, $0$ cacheline bouncing during parallel phase).
   - `sync.Mutex`: Severe lock contention degradation beyond 8 cores.
   - `chan T`: Significant channel contention and GC overhead under high message volume.

---

## 6. How to Run

```bash
# 1. Run the static commutativity & purity verification harness (30 test cases):
python test_verifier.py

# 2. Run the concurrent Go prototype & race detector:
go test -v -race

# 3. Run comparative benchmarks:
go test -bench=. -benchmem
```
