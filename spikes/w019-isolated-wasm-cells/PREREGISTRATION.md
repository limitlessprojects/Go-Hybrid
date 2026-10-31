# W-019 Pre-Registration: Isolated WASM Task Cells (Wire Boundary vs Cross-Task Alias Proofs)

- **Ledger ID**: `W-019`
- **Proposal**: Issue `#462` (*"[wildcard] Isolated WASM task cells: replace cross-task alias proofs with a wire boundary"*, proposed by `@Rangercook`)
- **Base Commit**: `422066bd873f7b206a27738e839b38aec82f53d5`
- **Sacred Cow Broken**: Single-address-space / no-VM runtime assumption and cross-task `may_share` alias proof obligations across `spawn` boundaries (replaced for opt-in `spawn cell` tasks by physical WebAssembly linear-memory disjointness and schema-checked wire serialization, while preserving intra-cell `may_share` borrow checking intact).

---

## 1. Thesis & Architectural Motivation

In Hybrid's baseline concurrency model, tasks execute inside a shared process address space. Passing a composite value across a `spawn` or channel boundary requires either:
1. Compile-time alias/uniqueness tracking (`may_share` loan sets, `sendable` checks, or linear capsule capabilities as in `W-001`..`W-004` and `W-022`), or
2. Deep-cloning object graphs inside the shared Go garbage-collected heap (`W-005`) or copying into off-heap message slabs (`W-014`).

**Wildcard `W-019` (`#462`)** asks whether an explicit opt-in coarse task tier (`spawn cell`) can eliminate cross-task alias proof obligations entirely by compiling each isolated task cell into its own distinct WebAssembly instance with private, non-shared linear memory. Because two WASM linear memories cannot address one another's bytes unless a host explicitly imports the same `WebAssembly.Memory` into both instances, cross-cell communication is physically restricted to serialized byte frames copied across a host coordinator boundary.

Inside a single cell, Hybrid's standard local `may_share` borrow checker continues to govern intra-cell references (`&T`, `&mut T`, slices) with zero change. Across `spawn cell` boundaries, cross-task alias analysis is replaced by a simpler, fail-closed admission check (`WireSendable` + module capability admission).

---

## 2. Pre-Registered Hypotheses & Falsifiers

All pass/fail criteria and falsifiers are locked prior to evaluation in accordance with Issue `#462` (*"How we would know it works (or doesn't)"*) and `MOONSHOT.md`.

| ID | Category | Claim / Falsifier Statement | Pass / Firing Threshold | Verification Artifact |
| :--- | :--- | :--- | :--- | :--- |
| **`P1`** | Snapshot Isolation | Sending a nested mutable tree (`OwnedTree` / `KindTree`) from Cell A to Cell B copies a serialized byte frame between distinct WASM linear memories; subsequent in-place mutation of the sender's value or linear memory leaves the receiver's decoded value and linear memory unchanged, and vice versa. | `0` cross-cell byte mutations observed across sequential and 8-worker `-race` runs; `assert_deep_equal` holds before and after cross-side writes. | `verify_model.mjs` (`verifyP1SnapshotIsolation`), `cell_admission_model.rs` (`test_p1_snapshot_isolation_across_cells`), `work/cell_test.go` (`TestP1_SnapshotIsolationAcrossCells`) |
| **`P2`** | Fail-Closed Admission | The compiler/coordinator boundary fails closed with deterministic diagnostics when given non-wire-sendable values (`HYB-E3901`) or unauthorized WASM module capabilities (`HYB-E3902`). | `100%` rejection of raw pointers (`*T`), mutable slice aliases, closures, channels, foreign handles, cyclic graphs, depth > `8`, frames > `64 KiB` (`HYB-E3901`), plus shared memory, host-imported memory, and ambient WASI imports (`HYB-E3902`). | `verify_model.mjs` (`verifyP2AndF1Admission`), `cell_admission_model.rs` (`test_p2_wire_and_module_admission_errors`), `work/cell_test.go` (`TestP2_FailClosedWireAndModuleAdmission`), `work/negative/*.go` |
| **`F1`** | Negative Control (Falsifier) | Distinct `WebAssembly.Instance` objects alone do **not** guarantee isolation if module admission (`HYB-E3902`) is bypassed and two instances import the same non-shared host `WebAssembly.Memory`. | **Falsifier Fires (Expected)**: Writing byte `0x41` / `0x42` into Instance A's linear memory is immediately visible at the same offset in Instance B when admission is bypassed. | `w019_cell_boundary_probe.py`, `verify_model.mjs` (`verifyP2AndF1Admission`), `work/cell_test.go` (`TestF1_Falsifier_BypassedHostMemoryImportAliases`) |
| **`P3`** | Persistent Command Reactor | A WASM cell compiled as a persistent command reactor (`hy_cell_step(ptr, len) -> status`) processes `1,000` sequential messages on a single live instance, preserving cell state across turns while resetting its per-turn scratch arena watermark. | `1,000` consecutive steps on `1` instance; monotonic state counter reaches `1,000`; leaked scratch arena bytes = `0` (`scratch_ptr == SCRATCH_BASE`). | `verify_model.mjs` (`verifyP3AndF2ReactorLifecycle`), `cell_admission_model.rs` (`test_p3_persistent_reactor_without_reinstantiation`), `work/cell_test.go` (`TestP3_PersistentReactorWithoutReentry`) |
| **`F2`** | Re-Entrant Callback & `_start` Falsifier | Standard WASI `_start` one-shot execution cannot service persistent turns without re-instantiation or runtime state corruption, and re-entrant host-to-guest callbacks while a guest frame is active violate single-threaded cell invariants. | **Falsifier Fires (Expected)**: Re-invoking `_start` or attempting re-entrant `hy_cell_step` while `frame_guard == 1` (`inFlight == true`) traps/fails closed with `HYB-E3903`. | `verify_model.mjs` (`verifyP3AndF2ReactorLifecycle`), `cell_admission_model.rs` (`test_f2_falsifier_reentrant_and_oneshot_rejected`), `work/cell_test.go` (`TestF2_Falsifier_ReentrantHostCallbackRejected`) |
| **`P4`** | Coarse-Workload Amortization | For coarse-grained tasks (`32 KiB` payload + `50,000` FNV-1a/XOR compute iterations) on a pre-warmed persistent reactor cell, wire serialization and boundary copy overhead is a small fraction of total task turn time. | Wire/boundary overhead share `< 15%` of total task turn time (`(wasmUs - natUs) / wasmUs < 0.15`). | `verify_model.mjs` (`runBenchmarkMatrix`), `work/bench_test.go` (`BenchmarkIsolatedCell_Coarse32KiB`) |
| **`F3`** | Fine-Grained Task Cost Falsifier | For fine-grained tasks (`64 B` message, `0` compute iterations), wire encoding and linear-memory copy dominate turn latency, and full Go WASM runtimes impose a multi-megabyte per-cell memory floor. | **Falsifier Fires (Expected)**: Tiny `64 B` task wire/boundary share `> 50%` of total turn time, falsifying `W-019` as a drop-in replacement for lightweight `spawn` tasks. | `verify_model.mjs` (`runBenchmarkMatrix`), `work/bench_test.go` (`BenchmarkIsolatedCell_Tiny64B`) |

---

## 3. Diagnostic Code Lattice (`HYB-E3901` .. `HYB-E3903`)

To ensure deterministic compiler and coordinator behavior, `W-019` defines three dedicated error codes:

1. **`HYB-E3901` (`ErrCodeWireNotSendable`)**:
   - Triggered statically or at coordinator admission when a value crossing a `spawn cell` boundary contains a raw pointer (`*T`), mutable slice alias, closure with environment, channel endpoint, foreign OS/runtime handle, cyclic object graph, nesting depth `> 8` (`MaxWireDepth`), collection length `> 32`, or serialized frame size `> 64 KiB` (`MaxWireFrameBytes`).
2. **`HYB-E3902` (`ErrCodeModuleViolation`)**:
   - Triggered when a candidate WASM cell module declares shared linear memory (`SharedArrayBuffer` / `threads`), imports linear memory from the host (`(import "host" "memory" ...)`), fails to export its own private `memory`, or imports ambient WASI/host capabilities (`wasi_snapshot_preview1::*`) outside the coordinator channel.
3. **`HYB-E3903` (`ErrCodeExecutionViolation`)**:
   - Triggered when a module uses one-shot `_start` execution instead of exporting persistent `hy_cell_step`, or when the host attempts re-entrant entry into a cell whose reactor frame guard (`frame_guard` / `inFlight`) is already active.

---

## 4. Experimental Protocol & Artifacts

1. **Real WASM Binary & Engine Verification (`spikes/w019-isolated-wasm-cells/verify_model.mjs`)**:
   - Synthesizes raw WebAssembly binary modules (`buildIsolatedCellWasm`, `buildImportedMemoryWasm`, `buildWasiImportWasm`), instantiates real `WebAssembly.Module`, `WebAssembly.Instance`, and `WebAssembly.Memory` objects, and executes `P1`–`P4` and `F1`–`F3`.
2. **Standalone Rust Admission & Borrow Model (`spikes/w019-isolated-wasm-cells/cell_admission_model.rs`)**:
   - Models `HYB-E3901`, `HYB-E3902`, `HYB-E3903`, and `LocalBorrowRegion::acquire_may_share` to prove intra-cell borrow rules remain unchanged while cross-cell alias proofs are replaced by `admit_wire_sendable`.
3. **Go Coordinator, Race Suite, Benchmarks & Compile-Fail Negatives (`spikes/w019-isolated-wasm-cells/work/`)**:
   - Implements `Live[R]` / `Moved[R]` zero-size phantom witnesses, `AdmitWireValue`, `AdmitModule`, `CellInstance.Step`, `Coordinator.SendSnapshot`, `-race` concurrency tests (`cell_test.go`), microbenchmarks (`bench_test.go`), and static type-checker rejection tests (`negative/raw_pointer_message.go`, `negative/reused_moved_witness.go`).
4. **Wasmtime Intake Probe (`spikes/w019-isolated-wasm-cells/w019_cell_boundary_probe.py`)**:
   - Preserves `@Rangercook`'s intake probe demonstrating snapshot survival and the `F1` host-memory import negative control under `wasmtime==49.0.0`.
