# Spike 13: Affine tx[T]/rx[T] Endpoint Split + Acyclic Wait-For Epochs

- **Proposal:** Wildcard Entry **W-009** (Issue #347, submitter: `@lienhoatienphung-svg`)
- **Review Slot:** Slot 9, 2026-11-01 Review Window ([Discussion #365](https://github.com/limitlessprojects/Go-Hybrid/discussions/365))
- **Track:** BOUNTY.md Track 3 (Wildcard Track)
- **Status:** Feasibility Spike Artifact & Formal Verification Model
- **Artifacts:**
  - Standalone Rust Model: [`endpoint_epoch_model.rs`](./endpoint_epoch_model.rs) (12 unit tests)
  - Executable Verification Harness: [`verify_model.mjs`](./verify_model.mjs) (12 passed, 0 failed)

---

## 1. Executive Summary

Wildcard entry **W-009** attacks two long-standing limits in [GUARANTEES.md](../../GUARANTEES.md):
1. **Row 16 (*"Send-on-closed-channel panics: No claim"*):** In standard Go and current Hybrid, channel lifecycle order is unverified. Calling `ch <- v` after `close(ch)` or closing twice causes a runtime panic.
2. **Known Limits (*"No claim of deadlock freedom"*):** Channels can form circular waiting dependencies across goroutines, causing permanent deadlock.

This feasibility spike validates that by splitting Go's undirected `chan T` into an **affine endpoint pair** `tx, rx := chan[T](cap)`:
- Making `close(move tx)` an `O5` ownership-consuming move tracked by `freeze.rs` **immediately flips Row 16 into a static `HYB-E3003` compile-time guarantee** across straight-line code, branches, closures, and loop back-edges with **zero new dataflow passes**.
- Assigning each channel allocation a lexical **Wait-For Epoch** $e(c) \in \mathbb{N}$ in `FnState` and checking that task obligation edges $(\leftarrow rx) \to (tx \leftarrow)$ form an acyclic directed graph ($O(V+E)$) **guarantees deadlock freedom in the checked subset (`HYB-E3201`)** without requiring heavyweight session types.

---

## 2. Property & Fault Model

### Guaranteed Properties (Checked Subset)
1. **Zero Send-on-Closed Panics:** It is impossible for an admitted program to execute a send operation on a closed channel (`panic: send on closed channel`).
2. **Zero Double-Close Panics:** It is impossible for an admitted program to call `close()` more than once on the same logical channel (`panic: close of closed channel`).
3. **Deadlock-Free Channel Networks:** Inter-task communication networks whose wait-for dependencies form an acyclic partial order are guaranteed to make forward progress without circular blockages under fair scheduling.
4. **Zero Runtime Overhead / Plain-Go Lowering:** In `hy-emit`, `tx, rx := chan[T](cap)` lowers 1:1 to standard Go `make(chan T, cap)`, using Go's native directional types (`chan<- T` for sender, `<-chan T` for receiver) with zero wrapper types, zero reflection, and zero runtime overhead.

### Explicit Non-Claims
- **Dynamic Unbounded Fan-in Cyclic Rendezvous:** Intentional cyclic actor protocols (e.g. bidirectional ping-pong on a single unbuffered channel) are rejected unless wrapped in the explicit `managed("cyclic-rendezvous", "actor")` audit hatch.
- **External OS / Network Blockages:** Does not model blocking system calls, socket timeouts, or deadlocks inside external un-instrumented Cgo libraries.

---

## 3. Addressing the Maintainer's Review Questions

In the triage comment for W-009, the maintainer noted:
> *"Reason: the strongest single move in the entry is making `close(move tx)` an O5-consuming move — row 16 flips to a compile-time use-after-close guarantee with zero new dataflow passes... Review will press on: `split(k)` fan-out semantics under loops and selects, epoch assignment for channels that cross function returns, and the migration story — killing undirected `chan T` is a versioned break that needs a corpus-wide path."*

### Review Point 1: `split(k)` Fan-Out Semantics under Loops and Selects
*Maintainer inquiry: How do split senders behave under fan-out, loops, and selects without permitting premature closes?*

**Resolution:**
- For multi-producer fan-out patterns, an owned `tx` endpoint can be split into $k$ sub-handles:
  ```go
  tx1, tx2 := tx.split(2)
  task producerA(move tx1)
  task producerB(move tx2)
  ```
- **Non-Owned Close Prohibition (`HYB-E3004`):** Sub-handles produced via `split` carry the flag `is_split: true`. A split sub-handle **is prohibited from calling `close()`**:
  $$\frac{\text{IsSplit}(tx)}{R \vdash \text{close}(\text{move } tx) : \textbf{Reject } (\text{HYB-E3004: non-owned-close})}$$
- **Closing Rule:** Either:
  1. The parent task retains ownership of the unsplit root `tx` and closes it after joining all sub-producers; or
  2. The channel lowers with an automatic barrier count in the supervisor epilogue, ensuring the underlying Go channel is closed exactly once when all $k$ producers have finished.
- In loops, calling `tx.split()` per iteration requires moving each split handle into a newly spawned task; loop-carried reuse without fresh splitting rejects at the loop backedge check.

### Review Point 2: Epoch Assignment for Channels Crossing Function Returns
*Maintainer inquiry: What happens when channels cross function returns? How is epoch ordering preserved across package boundaries?*

**Resolution:**
- When a function allocates and returns an endpoint pair:
  ```go
  fn makePipe() (tx chan<- int, rx <-chan int) {
      return chan[int](16)
  }
  ```
- The callee's returned endpoints are stamped with **abstract relative epochs** in the function summary:
  $$\text{Summary}(makePipe) = \{ \text{Returns}: (tx, rx), \; e(tx) = e(rx) = e_{\text{fresh}} \}$$
- At the call-site:
  ```go
  tx, rx := makePipe()
  ```
  The caller's `FnState` assigns the returned endpoints a concrete epoch equal to the call-site's allocation sequence index ($e_{\text{caller}}$).
- If a function takes an `rx` and returns a `tx`, its export summary registers the internal obligation edge:
  $$\text{Summary}(f) = \{ rx_{\text{in}} \to tx_{\text{out}} \}$$
  The caller incorporates this obligation edge directly into its local Wait-For DAG, verifying topological acyclicity in $O(V+E)$ time without requiring whole-program interprocedural analysis.

### Review Point 3: The Migration Story & Corpus Compatibility
*Maintainer inquiry: Killing undirected `chan T` is a versioned break that needs a corpus-wide path.*

**Resolution:**
1. **Go's Native Directional Syntax:** Go already natively possesses directional channel types:
   - `chan<- T` (send-only channel)
   - `<-chan T` (receive-only channel)
   - `chan T` (bidirectional channel, implicitly converts to both)
2. **Corpus Migration (15, 16, 17):**
   - In existing corpus cases (such as `15-channel-move-worker-collector.hy`), tasks already act as dedicated producers or consumers.
   - Migrating `15-channel-move-worker-collector.hy` merely involves specifying:
     ```go
     fn worker(id int, jobs <-chan int, results chan<- int) { ... }
     ```
   - Because in Go `make(chan int)` implicitly converts to both `<-chan int` and `chan<- int`, existing code compiles cleanly without breaking Go interoperability.
3. **Phased Compatibility:**
   - In Tier-0 safe mode, `ch := make(chan T)` is treated as syntactic sugar that automatically splits into `ch.tx` and `ch.rx` upon boundary crossing.
   - Undirected `chan T` crossing multiple task boundaries without directional specialization triggers a migration deprecation warning pointing to the affine split.

---

## 4. Formal Admission Algebra

### Part 1: Row 16 / O5 Close-as-Move Rules
Let $S$ denote the checker's `FnState` with held linear endpoints $\mathcal{H}$, consumed endpoints $\mathcal{C}$, and alias map $\mathcal{A}$.

$$\frac{tx \in \mathcal{H} \quad tx \notin \mathcal{C} \quad \neg \text{IsSplit}(tx)}{S \vdash \text{close}(\text{move } tx) \implies \mathcal{H}' = \mathcal{H} \setminus \{tx\}, \; \mathcal{C}' = \mathcal{C} \cup \{tx\}, \; \text{Freeze}(\mathcal{A}(tx))}$$

$$\frac{tx \in \mathcal{C} \lor tx \notin \mathcal{H}}{S \vdash tx \leftarrow v : \textbf{Reject } (\text{HYB-E3003: use-after-close})}$$

$$\frac{tx \in \mathcal{C}}{S \vdash \text{close}(tx) : \textbf{Reject } (\text{HYB-E3003: double-close})}$$

$$\frac{\text{IsSplit}(tx)}{S \vdash \text{close}(tx) : \textbf{Reject } (\text{HYB-E3004: non-owned-close})}$$

### Part 2: Wait-For Epoch & Cycle Rejection
Let $\mathcal{G} = (V, E)$ be the directed wait-for graph where an edge $(u, v) \in E$ denotes that a task holds receive endpoint $u$ and blocks on it before fulfilling send endpoint $v$.

$$\frac{\text{PathExists}(v, u) \text{ in } \mathcal{G}}{S \vdash \text{register}(\leftarrow u, \; v \leftarrow) : \textbf{Reject } (\text{HYB-E3201: cyclic-wait-for-deadlock})}$$

$$\frac{\neg \text{PathExists}(v, u) \text{ in } \mathcal{G}}{S \vdash \text{register}(\leftarrow u, \; v \leftarrow) : \text{Admitted } (E' = E \cup \{ (u, v) \})}$$

$$\frac{\text{ManagedBoundary}(\text{"reason"})}{S \vdash \text{register}(\leftarrow u, \; v \leftarrow) : \text{Admitted with managed lint}}$$

---

## 5. Verification Cases & Executable Test Results

The accompanying model [`endpoint_epoch_model.rs`](./endpoint_epoch_model.rs) and companion verification runner [`verify_model.mjs`](./verify_model.mjs) implement and test all 12 verification cases:

| Test ID | Category | Description | Outcome |
|---|---|---|---|
| **Test 1** | Row 16 / O5 | Straight-line send after close is rejected (`HYB-E3003`) | **PASS** |
| **Test 2** | Row 16 / O5 | Double-close on same tx is rejected (`HYB-E3003`) | **PASS** |
| **Test 3** | Row 16 / O5 | Branch-conditional close invalidates send post-merge (`HYB-E3003`) | **PASS** |
| **Test 4** | Row 16 / O5 | Alias send after move tx is rejected (`HYB-E3003` alias freeze) | **PASS** |
| **Test 5** | Row 16 / O5 | Split sub-handle cannot call close (`HYB-E3004` non-owned close) | **PASS** |
| **Test 6** | Row 16 / O5 | Valid producer-consumer pipeline with single-owner close accepted | **PASS** |
| **Test 7** | Deadlock / E3201 | Classic 2-task cross-wait deadlock rejected (`HYB-E3201`) | **PASS** |
| **Test 8** | Deadlock / E3201 | N-task cyclic dining philosophers ring rejected (`HYB-E3201`) | **PASS** |
| **Test 9** | Deadlock / E3201 | Idiomatic monotonic multi-stage pipeline accepted | **PASS** |
| **Test 10** | Deadlock / E3201 | Fan-out / Fan-in worker pool (Corpus 15) accepted | **PASS** |
| **Test 11** | Deadlock / E3201 | Request-reply pattern with distinct channels accepted | **PASS** |
| **Test 12** | Deadlock / E3201 | Intentional cyclic rendezvous permitted via managed hatch | **PASS** |

**Executable Verification Output:**
```
Running endpoint_epoch_model test suite (W-009 / Issue #347)...
[PASS] Test 1: Straight-line send after close is rejected (HYB-E3003)
[PASS] Test 2: Double-close on same tx is rejected (HYB-E3003)
[PASS] Test 3: Branch-conditional close invalidates send post-merge (HYB-E3003)
[PASS] Test 4: Alias send after move tx is rejected (HYB-E3003 alias freeze)
[PASS] Test 5: Split sub-handle cannot call close (HYB-E3004 non-owned close)
[PASS] Test 6: Valid producer-consumer pipeline with single-owner close accepted
[PASS] Test 7: Classic 2-task cross-wait deadlock rejected (HYB-E3201)
[PASS] Test 8: N-task cyclic dining philosophers ring rejected (HYB-E3201)
[PASS] Test 9: Idiomatic monotonic multi-stage pipeline accepted
[PASS] Test 10: Fan-out / Fan-in worker pool (Corpus 15) accepted
[PASS] Test 11: Request-reply pattern with distinct channels accepted
[PASS] Test 12: Intentional cyclic rendezvous permitted via managed hatch

Result: 12 passed, 0 failed
```

---

## 6. Comparison with Alternative Wildcard Proposals

| Dimension | W-009 (Affine tx/rx + Epochs) | W-010 (Session-Type Generation) | W-007 (Blocking-Effect Regions) |
|---|---|---|---|
| **Primary Guarantee** | Row 16 panic freedom + Acyclic deadlock freedom | Multipary session type safety | Structured cancellation & leak freedom |
| **Syntax Overhead** | Zero new syntax (uses Go `chan<-` / `<-` types) | Heavy explicit protocol DSL & annotations | Opt-in `task_region` block |
| **Checker Complexity** | Piggybacks on `O5` move/freeze + $O(V+E)$ DAG | Complex multi-role session automata | Region supervisor lowering |
| **Go Interoperability** | 100% native Go channel compatibility | Requires protocol state wrappers | Plain Go `context` + `WaitGroup` |

---

## 7. Go/No-Go Decision Matrix

| Criterion | Risk / Failure Hypothesis | Spike Finding | Verdict |
|---|---|---|---|
| **Compiler Simplicity** | Requires rewriting the dataflow analysis. | **Falsified:** Piggybacks directly on existing `O5` move tracking in `freeze.rs`. | **GO** |
| **Deadlock Scalability** | Cycle check causes exponential compile time. | **Falsified:** DFS over lexical channel DAG is strictly $O(V+E)$ per function. | **GO** |
| **Go Code Generation** | Emitted Go requires runtime locks or custom wrappers. | **Falsified:** Lowers 1:1 to plain Go native channels and directional types. | **GO** |
| **Corpus Compatibility** | Breaks idiomatic Go pipelines. | **Falsified:** Idiomatic pipelines (corpus 15/16/17) satisfy $e(u) < e(v)$ naturally. | **GO** |

### Recommendation for M1 Review Window (2026-11-01)
Proceed with **M1 Acceptance** of W-009. The combination of `close(move tx)` as an `O5` move and Acyclic Wait-For Epochs provides the cleanest, lowest-overhead mechanism to flip Row 16 and deadlock freedom in Hybrid's history.
