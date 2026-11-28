# W-4699 Spike Results — Deterministic Artifact Manifests (#4699)

## Summary

This spike implements the M1/M2 feasibility model for **Wildcard #4699:
Deterministic artifact manifests with ordinary Go task scheduling**. It is a
pure, deterministic Go reference model plus a static admission pass. It does
**not** implement the Hybrid checker, real Go parallelism, or filesystem
commits; it provides the coordinator semantics and a battery of falsifying tests
that would catch a violation of the determinism guarantee.

## Pre-Registered Phases & Falsifiers

| Phase / ID | Pre-Registered Obligation & Falsifier | Verification Test | Status |
|---|---|---|---|
| **M1 — Admission** | Restrict the experiment to transitively checked functions, deeply immutable values and finite ordered child lists. Reject ambient reads/writes, opaque calls, ordinary channel operations and explicit managed escapes with typed diagnostics `HYB-E4701..HYB-E4705`. | `TestM1_AdmissionRejectsOpaqueCallAndAmbient` | **PASS** |
| **M1 — Four schedules** | A root `1` produces children `2` and `3`; every value below `4` produces children `2*n` and `2*n+1`. Four different ready-task selection policies (FIFO, LIFO, DepthFirst, Random) must all produce the same manifest order `1, 2, 4, 5, 3, 6, 7`. | `TestM1_FourSchedulesProduceSameManifest` | **PASS** |
| **M1 — Two failures** | A two-failure fixture must produce the same failure order under FIFO and LIFO, ordered by numeric path. | `TestM1_TwoFailuresSameOrderFIFOAndLIFO` | **PASS** |
| **M1 — Non-finite** | The model rejects NaN/Inf float payloads before staging (`HYB-E4707`). | `TestM1_RejectsNonFiniteData` | **PASS** |
| **M1 — Budget** | A group over its task or byte budget produces **no manifest** (`ErrBudgetExhausted`, nil manifest). | `TestM1_RejectsGroupOverBudget` | **PASS** |
| **M2 — Randomized seeds** | Compare the built fixture under single-worker (FIFO) and randomized multi-worker schedules across 100 fixed seeds. Hash only the documented manifest bytes. Falsify the guarantee if any admitted fixture emits different bytes. | `TestM2_RandomizedMultiWorkerSeedsHashOnlyManifestBytes`, `TestM2_SingleWorkerMatchesMultiWorker` | **PASS** |
| **Path ordering** | Numeric path ordering with correct prefix handling. | `TestPathOrdering` | **PASS** |

## Reproduction

```bash
cd spikes/w4699-artifact-manifest/work
CGO_ENABLED=0 go test -v -count=1 ./...
CGO_ENABLED=0 go test -bench=Benchmark -benchmem -run=^$ ./...
```

## What this spike does NOT do (honest scope)

- It does not implement the Hybrid compiler checker or lower `artifact_group`
  to Go. The `.hy` fixtures under `hy/` are illustrative; the admission pass is
  a Go reference (`CheckWorkerAdmission`) operating on source snippets.
- It does not run real Go goroutines; the coordinator is a deterministic
  single-threaded scheduler that *simulates* legal schedules.
- It does not commit files to a filesystem; artifacts are staged as immutable
  byte slices and materialization is left at the consumer boundary.
- Streaming output, arbitrary Go imports, wall-clock cancellation inside the
  checked region, and first-completer error handling are out of scope by design
  (per the issue's cost-honesty section).

## Test Output

```
=== RUN   TestM1_FourSchedulesProduceSameManifest
--- PASS: TestM1_FourSchedulesProduceSameManifest (0.00s)
=== RUN   TestM1_TwoFailuresSameOrderFIFOAndLIFO
--- PASS: TestM1_TwoFailuresSameOrderFIFOAndLIFO (0.00s)
=== RUN   TestM1_RejectsNonFiniteData
--- PASS: TestM1_RejectsNonFiniteData (0.00s)
=== RUN   TestM1_RejectsGroupOverBudget
--- PASS: TestM1_RejectsGroupOverBudget (0.00s)
=== RUN   TestM1_AdmissionRejectsOpaqueCallAndAmbient
--- PASS: TestM1_AdmissionRejectsOpaqueCallAndAmbient (0.00s)
=== RUN   TestM2_RandomizedMultiWorkerSeedsHashOnlyManifestBytes
--- PASS: TestM2_RandomizedMultiWorkerSeedsHashOnlyManifestBytes (0.00s)
=== RUN   TestM2_SingleWorkerMatchesMultiWorker
--- PASS: TestM2_SingleWorkerMatchesMultiWorker (0.00s)
=== RUN   TestPathOrdering
--- PASS: TestPathOrdering (0.00s)
PASS
ok  	hybrid/spikes/w4699-artifact-manifest	0.004s
```