# Spike 18 Results: Static-Rate Task Networks & Certified Periodic Go Kernels

- **Proposal:** Wildcard Entry **W-018** (Issue #378, original proposal & prototype: `@untalsix`)
- **Review Slot:** Slot 18, 2026-11-01 Review Window ([Discussion #365](https://github.com/limitlessprojects/Go-Hybrid/discussions/365))
- **Track:** BOUNTY.md Track 3 (Wildcard Track)
- **Status:** Complete Feasibility Spike Artifact, Schedule Certificates, and Runtime FIFO Guard Verification
- **Artifacts:**
  - Independent SDF Planner & Certificate Verifier: [`planner.py`](./planner.py)
  - Unit Test Suite: [`test_planner.py`](./test_planner.py) (15 tests passed)
  - Code Generator: [`generate.py`](./generate.py) (emits 4 Go kernels, 8 Go tests, and JSON certificates)
  - Runtime FIFO Guard Verifier: [`check_runtime_buffers.py`](./check_runtime_buffers.py) (9 pop + 9 push dynamic guards, 10,000 periods, marking checks, negative mutation control)
  - Generated Kernels & Tests: [`generated.go`](./generated.go), [`generated_test.go`](./generated_test.go)
  - Benchmark Harness: [`benchmark_test.go`](./benchmark_test.go)
  - Replayable Schedule Certificates: [`certificates.json`](./certificates.json)

---

## 1. Executive Summary

Wildcard entry **W-018** attacks the standard assumption that every concurrent task and channel in an admitted program must survive as an OS goroutine and Go channel in emitted code. For **closed, fixed-rate, scalar-value streaming networks** (Synchronous Data Flow, Lee & Messerschmitt 1987), this spike proves that the network can be compiled away entirely into a deterministic, sequential, allocation-free Go kernel with statically bounded ring buffers.

The compilation produces a **replayable schedule certificate** containing:
1. Fractional production/consumption rates and initial markings.
2. A minimal periodic repetition vector derived from topology balance equations.
3. A single-appearance or unrolled periodic execution schedule.
4. Exact FIFO queue capacities derived from token high-water marks during simulated replay.

An independent verifier validates the witness: rejecting buffer underflow, buffer overflow, rate inconsistencies, and uninitialized cycles. The resulting admitted kernel guarantees **bounded FIFO token storage** and **zero channel-token deadlocks** under the certified periodic schedule, executing with zero heap allocations per step.

---

## 2. Environment & Toolchain Verification

- **Host Environment:** Darwin/arm64, Apple A18 Pro; validated on Windows 11 under Python 3.13.7.
- **Go Toolchain:** Go 1.26.9, verified against its published SHA-256:
  `f9bb7c0a02506c5d9bf0d1eb1f7ee6c7684f844ae49558308a0427b830e022cc`.
- **Python Toolchains:** Tested across Python 3.9, 3.12, and 3.13 standard libraries with zero external dependencies.

---

## 3. Correctness & Verification Suite

### Python Planner & Independent Replay (`test_planner.py`)
Command: `python3 -m unittest -v test_planner.py`
**Result: 15 passed, 0 failed (0.021s)**

Verified behaviors include:
- **Topology Balance Equations:** Correct minimal non-trivial repetition vectors across multirate pipelines, fan-out/fan-in joins, and cycles.
- **Hand-Derived Reference Oracles:** Checksums, final token values, and queue states match analytical models across all four test fixtures.
- **Initialized Feedback Loops:** Proper simulation of 64-bit wrapping arithmetic and cycle initialization.
- **Admission Rejections:**
  - Rejection of uninitialized cycles (deadlocks) with `ValueError: no enabled`.
  - Rejection of rate mismatches with `ValueError: inconsistent`.
  - Rejection of invalid rates (non-positive, non-integer, exceeding 4096 limit).
  - Rejection of unmodeled/foreign side effects.
  - Rejection of forged schedules violating token availability (`underflow`).
  - Rejection of incomplete periodic schedules.
  - Rejection of forged buffer capacities and insufficient token payload budgets.
  - Rejection of invalid initial token payloads.
  - Rejection of actor names containing injected Go syntax.

### Go Kernel Generation & Zero Allocation Validation (`generated_test.go`)
Command: `go test -v ./...`
**Result: 9 passed, 0 failed**

- Four Python reference oracle tests: `TestKernel0Reference` through `TestKernel3Reference` match expected checksums and state.
- Four zero-allocation tests: `testing.AllocsPerRun(10000, ...)` verifies **0 allocations/step** across all four generated kernels.
- One behavioral equivalence test: `TestPersistentChannelMatchesStatic` verifies that `Kernel0` produces the identical result (`150015000`) as a concurrent goroutine/channel pipeline over 10,000 values.

### Certified Buffer Capacities (Four Fixtures)
- **Kernel 0 (Simple Pipeline):** 16 payload bytes (two 8-byte uint64 queues of capacity 1).
- **Kernel 1 (Multirate 2:3:1):** 64 payload bytes (queue 0 capacity 6, queue 1 capacity 2).
- **Kernel 2 (Diamond Join):** 24 payload bytes (three queues of capacity 1).
- **Kernel 3 (Feedback Cycle):** 16 payload bytes (two queues of capacity 1, initialized with 1 feedback token).

*(Note: These figures represent token payload bytes. They exclude ring indices, actor state variables, struct padding, and runtime stacks.)*

---

## 4. Runtime FIFO Guard Experiment

To provide independent runtime validation of the emitted ring-buffer indexing, [`check_runtime_buffers.py`](./check_runtime_buffers.py) instruments every queue method with dynamic assertion guards:
- `pop(i)`: Panics with `buffer underflow` if $n_i \le 0$.
- `push(i)`: Panics with `buffer overflow` if $n_i \ge \text{capacity}_i$.

### Results:
1. **Dynamic Stability:** All 4 kernels ran with dynamic guards for **10,000 consecutive periods** each (9 pop and 9 push methods guarded). Zero underflow or overflow panics occurred.
2. **Periodic Marking Invariance:** The occupancy $n_j$ of every queue was checked after every period and strictly equaled the initial marking.
3. **Negative Control Verification:** Deliberately removing a pop decrement (`k.n0--`) caused the negative control test to immediately trigger `panic: buffer overflow` on the next period, confirming that the runtime guards actively detect and reject illegal buffer states.

---

## 5. Raw Timing & Benchmark Results

Command: `go test -run '^$' -bench . -benchmem -benchtime=200ms -count=5`

```text
goos: darwin
goarch: arm64
pkg: example.org/hybrid-static-network-spike
cpu: Apple A18 Pro
BenchmarkStaticPipeline-6      98022068   2.458 ns/op   0 B/op   0 allocs/op
BenchmarkStaticPipeline-6      89452105   2.268 ns/op   0 B/op   0 allocs/op
BenchmarkStaticPipeline-6     100000000   2.292 ns/op   0 B/op   0 allocs/op
BenchmarkStaticPipeline-6     100000000   2.344 ns/op   0 B/op   0 allocs/op
BenchmarkStaticPipeline-6     100000000   2.316 ns/op   0 B/op   0 allocs/op
BenchmarkChannelPipeline-6     1234926 195.2 ns/op     0 B/op   0 allocs/op
BenchmarkChannelPipeline-6     1201070 199.6 ns/op     0 B/op   0 allocs/op
BenchmarkChannelPipeline-6     1000000 201.1 ns/op     0 B/op   0 allocs/op
BenchmarkChannelPipeline-6     1000000 202.7 ns/op     0 B/op   0 allocs/op
BenchmarkChannelPipeline-6     1000000 207.7 ns/op     0 B/op   0 allocs/op
BenchmarkPlainSequential-6    126654616   1.892 ns/op   0 B/op   0 allocs/op
BenchmarkPlainSequential-6    127008178   1.883 ns/op   0 B/op   0 allocs/op
BenchmarkPlainSequential-6    128510299   1.865 ns/op   0 B/op   0 allocs/op
BenchmarkPlainSequential-6    127455594   1.885 ns/op   0 B/op   0 allocs/op
BenchmarkPlainSequential-6    120718197   1.974 ns/op   0 B/op   0 allocs/op
PASS
ok example.org/hybrid-static-network-spike 3.723s
```

### Analysis:
- **Static Pipeline:** **2.316 ns/op** (median)
- **Persistent Channel Round Trip:** **201.1 ns/op** (median)
- **Plain Sequential Loop:** **1.885 ns/op** (median)

The static ring-buffer kernel executes within **~23% of raw sequential arithmetic** and is **~87x faster than persistent unbuffered channel communication**, while preserving identical mathematical output. The speedup comes from eliminating goroutine context switching, scheduler preemption, and channel mutex locking for networks whose communication schedule is statically knowable at compile time.

---

## 6. Honest Scope & Clarification of Claims

Per the author's (@untalsix) clarification:
1. **Certificate Semantics:** The certificates in `certificates.json` are **replayable schedule witnesses with high-water marks for the submitted schedule**. They are not cryptographic digital signatures and do not claim global mathematical optimality of buffer sizes across all theoretical schedules.
2. **What Is Proved:**
   - For an admitted fixed-rate network with a valid schedule certificate, the kernel executes with zero channel-token deadlocks and bounded ring buffer storage.
   - Step execution is allocation-free when actor state is scalar/closed.
3. **What Remains Outside Admission (Unverified):**
   - Dynamic actor spawning, select-dependent firing rates, and cancellation.
   - Unbounded foreign calls, blocking I/O, and non-scalar payload graphs.
   - Total process memory or stack bounds (the guarantee is strictly FIFO token storage).

---

## 7. Go/No-Go Decision Matrix

| Dimension | Risk / Failure Criterion | Spike Result | Verdict |
|---|---|---|---|
| **SDF Solvability** | Fractional rates lead to unbounded accumulation. | **Falsified:** Balance equations find minimal repetition vector or fail closed. | **GO** |
| **Deadlock Freedom** | Uninitialized cycles hang the schedule. | **Falsified:** Topological simulator rejects uninitialized feedback before admission. | **GO** |
| **Zero Allocations** | Ring buffers leak into heap allocations. | **Falsified:** `testing.AllocsPerRun` confirms 0 allocations/step across all fixtures. | **GO** |
| **Runtime Bounds** | Emitted indexing overflows during long runs. | **Falsified:** Guarded run completed 10,000 periods with 0 overflows/underflows. | **GO** |

### Recommendation for M1 Review Window (2026-11-01)
Proceed with **M1 Acceptance** of W-018. Static-rate task networks provide a viable, verifiable mechanism to compile streaming actor subgraphs into allocation-free Go ring-buffer kernels.
