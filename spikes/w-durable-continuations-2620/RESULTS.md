# W-2620 Spike Results — Compiler-Generated Durable Continuations

**Proposal:** #2620 | **Author:** `@ket4min-susan` | **Verifiers:** `python3 spikes/w-durable-continuations-2620/durable_continuations_verifier.py --check --live-go` & `pytest spikes/w-durable-continuations-2620/tests/test_w2620_spike.py`

## 1. Pre-Registered Gates & Negative Controls (`P1`–`P3`, `F1`–`F5`)

| ID | Gate | Measured Result | Verdict |
| :--- | :--- | :--- | :--- |
| **P1** | Static `@[durable_region]` admission (`HYB-E3620-*`) admits finite bounded-FIFO pipelines & fan-out regions; rejects foreign I/O, clocks/RNG, pointers, closures, `managed`, unbounded channels, and loop spawns | `01` & `02` accepted (`0` diags); `03` rejected across all 7 `HYB-E3620-*` rules | **PASS** |
| **P2** | Exhaustive fault injection before & after every scheduler transition and mid-commit boundary on Producer-Transformer-Reducer (`ptr_v1`) and 3-worker Fan-Out/Fan-In (`fanout_v1`) | 100% terminal result & queue parity (`result=4504`), `30/43` steps saved on 75%-progress crash (**69.8% recovery work reduction** vs rerun-from-input) | **PASS** |
| **F1–F5** | Negative controls: omitted queue item (`F1`), producer-only cursor advance (`F2`), invalid continuation PC/wait (`F3`), stale `build_id`/`schema_version` (`F4`), and mid-write commit crash (`F5`) | All 5 corruptions deterministically rejected (`ERR_TORN_CUT`, `ERR_INVALID_CONTINUATION`, `ERR_BUILD_IDENTITY_MISMATCH`, `ERR_CORRUPT_CUT`) | **FIRED** |
| **P3** | Complexity & overhead vs Rerun-from-Input and Hand-Written Checkpoint (`work/durable_test.go`) | Snapshot size: **`669 B`**; 20-line `.hy` region vs ~145-line manual state machine (**7.2x source reduction**) | **PASS** |
