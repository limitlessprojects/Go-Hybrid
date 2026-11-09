# W-4176 Spike Results — Exhaustive Schedule Enumeration & Replayable Certificate (`.hcert`) (#4176)

## 1. Pre-Registered Falsifier Outcomes (F1–F6)

| Falsifier / Phase | Pre-Registered Obligation & Threshold (`PREREG.md`) | Empirical Measurement (`tests/checker-corpus/` + 10k Suite) | Verdict |
|---|---|---|---|
| **F1 (Comment-Blind Corpus Agreement & `managed` Soundness Signal)** | 100% agreement on all non-`ACCEPT-MANAGED` programs in `tests/checker-corpus/` (`137 / 137`), plus exact discrimination between `02-shared-map-managed.hy` (unprotected map write inside `managed`) and `18-pool-shared-struct-managed.hy` (`sync.mutex`-guarded struct inside `managed`) computed **without consulting `// EXPECTED-VERDICT:` headers**. | **`137 / 137` (`100.0%`)** agreement on non-managed programs (`45 ACCEPT`, `92 REJECT`).<br>On the 2 `ACCEPT-MANAGED` programs:<br>• **`02-shared-map-managed.hy` → `RACE_DETECTED`** (disagreement signal fired: emits 4-step vector-clock counterexample trace showing concurrent `ExclusiveWrite` on `counts` with `mutex=None` and incomparable vector clocks `vc_T1=[1,0]`, `vc_T2=[0,1]`).<br>• **`18-pool-shared-struct-managed.hy` → `CERTIFIED`** (`stats.mu.lock()` / `stats.mu.unlock()` holds across all reachable interleavings). | **PASS (Disagreement Signal Confirmed)** |
| **F2 (Strict Unclamped Admission Bound $\ge 60\%$)** | Admitted bounded subset ($N_{\text{tasks}} \le 4$, $\sum \text{Depth}(\text{chan}) \le 2$, $|S| \le 10^5$) evaluated on **unclamped** constant-propagated AST parameters must cover $\ge 60\%$ of the 139-program corpus. | **`130 / 139` (`93.53%`)** strictly admitted within the raw unclamped bound ($N_{\text{tasks}} \le 4, \sum \text{Depth} \le 2$); the **`9 / 139`** programs with raw channel buffer sum $> 2$ (`01`, `02`, `03`, `04`, `15`, `16`, `17`, `18`, `33`) are explicitly reported as `admitted_in_bound = False` (`OUTSIDE_RAW_BOUND`) and evaluated under bounded symmetry reduction. | **PASS (`93.53% >= 60%`)** |
| **F3 (Generation $< 150\text{ ms}$ & Replay $< 15\text{ ms}$)** | Enumeration + `.hcert` emission $< 150\text{ ms}$ per program; independent linear-time $O(|V_{\text{cert}}| + |E_{\text{cert}}|)$ replay verification $< 15\text{ ms}$ per program ($< 2000\text{ ms}$ CI budget). | • Max per-program generation: **`0.725 ms`** (total across all 139 programs: **`6.18 ms`**)<br>• Max per-program `.hcert` replay: **`0.361 ms`** (total across all 46 certified programs: **`3.10 ms`**) | **PASS** |
| **F4 (10,000 Synthetic Schedule Variations)** | `0` False Negatives across `5,000` seeded racy schedule variations and `0` False Positives across `5,000` seeded safe bounded idioms (including vector-clock happens-before channel handoffs). | • Injected racy variations caught: **`5,000 / 5,000` (`FN = 0`)**<br>• Safe idioms certified & replayed: **`5,000 / 5,000` (`FP = 0`)** | **PASS** |
| **F5 (7 Negative Certificate Tamper Controls incl. Successor Closure & Source Binding)** | Tampering with `.hcert` schema version, SHA-256 state-space digest, transition reachability closure, state count header, injecting an unsynchronized write, **omitting an enabled outgoing transition (`HYB-CERT-E009`)**, or **mismatching `source_sha256` (`HYB-CERT-E010`)** must fail closed. | **`7 / 7` negative tamper controls failed closed** with deterministic `HYB-CERT-E001..E010` diagnostics (even when the attacker recomputes `sha256_state_digest` after pruning a racy outgoing transition). | **FIRED AS PREDICTED** |
| **F6 (Comment-Blindness Control)** | Forging `// EXPECTED-VERDICT: ACCEPT` on a racy program (`02-shared-map-managed.hy`) must still produce `RACE_DETECTED`. | `parse_hy_ast` strips all `//` comments prior to `ProgramAST` construction; `enumerate_hy_program` still outputs `RACE_DETECTED`. | **PASS** |

---

## 2. Key Architectural Findings

1. **Resolving the `managed` Escape-Hatch Blind Spot (`02-shared-map-managed.hy` vs. `18-pool-shared-struct-managed.hy`)**:
   - In Tier-0 static checking, both `02-shared-map-managed.hy` and `18-pool-shared-struct-managed.hy` receive the identical verdict `ACCEPT-MANAGED`, even though `02` crashes at runtime with a Go concurrent map write fatal while `18` is data-race-free under `stats.mu.lock()`.
   - By modeling both vector clocks (`V_t[u]`) and mutex lock sets (`locks_held` + per-access `mutex_held`) in the LTS state vector, the enumerator **certifies `18-pool-shared-struct-managed.hy`** (`sha256` verified `.hcert`) while **refuting `02-shared-map-managed.hy`** with the minimal counterexample witness schedule:
     ```text
     s0: Init(raw_tasks=4, raw_chan_depth=4, vc=[[0,0],[0,0]])
     s1: T1 -> Spawn(arg='counts', alias_mode=MutableRef, vc_T1=[1,0])
     s2: T2 -> Spawn(arg='counts', alias_mode=MutableRef, vc_T2=[0,1])
     s3 [VIOLATION I_DRF]: O_s3('counts') = {(T1, ExclusiveWrite, mutex=None, vc=[1,0]), (T2, ExclusiveWrite, mutex=None, vc=[0,1])} [concurrent: !(vc_T1 <= vc_T2)]
     ```
2. **Complete Successor Closure (`HYB-CERT-E009`) + Source Binding (`HYB-CERT-E010`)**:
   - A sound state-space certificate verifier cannot rely solely on forward reachability (`visited == state_ids`) and a self-hash (`sha256_state_digest`), because a prover could drop an enabled outgoing transition leading to a racy state and re-hash the truncated graph.
   - Recording `enabled_tasks` per state and verifying that every enabled task has a corresponding outgoing transition edge (`HYB-CERT-E009`), alongside binding `source_sha256` (`HYB-CERT-E010`), closes this soundness gap in $O(|V_{\text{cert}}| + |E_{\text{cert}}|)$ time.

---

## 3. Reproduction

```bash
# Python Corpus Enumerator + 10,000-Variation Verification Suite (F1-F6)
python3 spikes/w4176-schedule-enumeration/enumerator.py
python3 spikes/w4176-schedule-enumeration/tests/test_schedule_enumeration.py

# Go Native Certificate Replay, Multi-GOMAXPROCS Execution & Benchmark Suite
cd spikes/w4176-schedule-enumeration
CGO_ENABLED=0 go test -v -count=1 ./...
CGO_ENABLED=0 go test -bench=Benchmark -benchmem ./...
```
