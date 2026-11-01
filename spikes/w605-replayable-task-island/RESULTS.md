# W-605 Spike Results — Replayable Task Island (#605)

## Pre-Registered Phases & Falsifiers (Phases 1–4)

| Phase / ID | Pre-Registered Obligation & Falsifier | Verification Test | Status |
|---|---|---|---|
| **Phase 1 (Admission & Lowering)** | Define a closed `task` / `chan` / `select` / virtual-timer subset with stable event IDs and reject unmediated sources of nondeterminism (`time.now()`, `rand.`, foreign callbacks, external I/O, `managed` blocks, unordered map iteration, raw goroutines/channels) with typed diagnostics `HYB-E3601..HYB-E3604`. | `TestPhase1_AdmissionPassRejectsUnmediatedNondeterminism` | **PASS** |
| **Phase 2 (Replay Completeness across 3 Workloads)** | Construct all 3 reference workloads from `#605`:<br>1. Two producers racing to send to one consumer (`W1_TwoProducersRacing`)<br>2. Multiple simultaneously ready `select` arms (`W2_MultiReadySelect`)<br>3. Task graph with virtual timers (`W3_VirtualTimers`)<br>Generate traces under 32 controlled scheduling seeds (`1..32`). Replaying each trace against a fresh coordinator initialized with an adversarial PRNG seed (`999999`) must reproduce the exact event sequence, output transcript, and 32-byte SHA-256 final state digest (`96 / 96` deterministic replays). | `TestPhase2_ThreeWorkloadsRecordAndReplayCompleteness` | **PASS** |
| **Phase 3 (Negative Controls & Tamper Falsifiers)** | Deliberately alter an `EventID`, swap a `select` `ChoiceSlot`, truncate a trace before island quiescence, mismatch the program/input contract version, or tamper with the final state digest. Every negative control must fail closed (`HYB-E3605..HYB-E3608`). | `TestPhase3_NegativeControlsTamperedTracesFailClosed` | **FIRED AS PREDICTED** |
| **Phase 4 (Cost Evaluation)** | Measure per-island Record and Replay latency, memory allocations, and binary trace storage cost (`EncodeBinary`). | `BenchmarkWorkload1Record` / `BenchmarkWorkload1Replay` | **`16 B / event` (+ `48 B` header); Record `7.51 µs/op`, Replay `6.89 µs/op`** |

## Reproduction

```bash
cd spikes/w605-replayable-task-island/work
CGO_ENABLED=0 go test -v -count=1 ./...
CGO_ENABLED=0 go test -bench=Benchmark -benchmem ./...
```
