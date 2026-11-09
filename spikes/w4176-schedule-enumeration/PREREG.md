# W-4176 Pre-Registration — Exhaustive Schedule Enumeration & Replayable Certificates (#4176)

## 1. Hypothesis & Premise Challenged

This spike pre-registers and evaluates the **W-4176 (`#4176`)** wildcard hypothesis:
1. **Static Dataflow vs. Explicit-State Enumeration:** For bounded concurrent regions ($N_{\text{tasks}} \le k_1$, $\sum \text{Depth}(\text{chan}) \le k_2$, finite memory mutation roots), Tier-0's static sendability proof can be demoted to a fast pre-filter while an **explicit-state Labeled Transition System (LTS) schedule enumerator with Sleep-Set Partial Order Reduction (POR)** serves as the soundness authority.
2. **Replayable Certificate Artifact (`.hcert`) vs. Unchecked Trust:** Instead of trusting compiler-internal dataflow passes, the enumerator emits a compact, machine-checkable `.hcert` certificate containing the reachable state-vector graph, per-state alias ownership maps $O_s: \text{Location} \to \mathcal{P}(\text{TaskID} \times \{\text{SharedRead}, \text{ExclusiveWrite}\})$, and a SHA-256 state-space digest verifiable by an independent linear-time $O(|V_{\text{cert}}| + |E_{\text{cert}}|)$ replay validator.

---

## 2. Pre-Registered Admission Contract & Invariant Specification

- **Admission Bound ($\mathcal{A}$):**
  - $N_{\text{tasks}} \le 4$ active concurrent tasks per region
  - $\sum \text{Depth}(\text{chan}) \le 2$
  - State budget cap $|S_{\text{max}}| \le 10^5$ states (and extended $10^7$ ceiling study)
  - Regions using unbounded dynamic task spawning in open loops or unmodeled foreign C/Go callbacks outside `{mutex, atomic, actor}` are classified as `OUTSIDE_BOUND` (fallback to Tier-0 static checker).
- **Safety Invariant ($\mathcal{I}_{\text{DRF}}$):**
  $$\forall s \in S_{\text{reachable}}, \forall l \in \text{Location}: \neg \Big( \exists t_1 \neq t_2 \text{ s.t. } (t_1, \text{ExclusiveWrite}) \in O_s(l) \land (t_2, m) \in O_s(l) \Big)$$
  Any reachable state violating $\mathcal{I}_{\text{DRF}}$ aborts `.hcert` emission and emits a deterministic **Counterexample Interleaving Witness Trace** (exact task step + channel rendezvous sequence from $s_0$ to the racy state $s_{\text{race}}$).

---

## 3. Pre-Registered Falsifiers & Decision Thresholds (Locked Before Commit 2)

| Falsifier ID | Criterion | Pass / Refutation Threshold |
|---|---|---|
| **F1 (Corpus Agreement & `managed` Honesty Signal)** | Enumerate all **139 `.hy` programs** in `tests/checker-corpus/`. Compare the enumerator's safety decision against `EXPECTED-VERDICT`. | **100% agreement (`137 / 137`)** on all non-`ACCEPT-MANAGED` programs (`32 ACCEPT`, `105 REJECT` including post-MVP corpus additions). On the `2 ACCEPT-MANAGED` programs (`02-shared-map-managed.hy` vs. `18-pool-shared-struct-managed.hy`), when mutex synchronization is modeled, the enumerator must **refute `02-shared-map-managed.hy`** (emitting the exact 2-task concurrent map write trace documented in `02`'s `RUNTIME-HAZARD` header) while **certifying `18-pool-shared-struct-managed.hy`** (`sync.Mutex` exclusion holds across all schedules). |
| **F2 (Admission Bound Coverage $\ge 60\%$)** | Measure the percentage of the 139 corpus programs whose concurrency topologies fit inside the bounded admission contract ($\le 10^5$ states). | **Falsified if $< 60.0\%$** of the corpus is admitted within the state budget. Target: $\ge 85\%$ admitted. |
| **F3 (Throughput & Replay Cost)** | Measure explicit-state enumeration + `.hcert` emission time and independent linear-time `.hcert` replay verification time across the corpus. | Enumeration + certificate emit must complete in **$< 150\text{ ms}$ per program** ($< 2000\text{ ms}$ across the entire 139-program corpus), and independent `.hcert` replay verification must complete in **$< 15\text{ ms}$ per program** (well below the $2\text{ s}$ CI ceiling). |
| **F4 (10,000 Synthetic Schedule Variations & Tamper Controls)** | Run 10,000 seeded synthetic concurrent regions (5,000 injected racy patterns with non-zero channel buffer offsets / post-send alias writes; 5,000 safe move-send / mutex-guarded / fan-in patterns) + 5 negative certificate tamper controls. | **0% False Negatives** (`5,000 / 5,000` racy variations caught with valid counterexample traces), **0% False Positives** (`5,000 / 5,000` safe variations certified), and **5 / 5 tampered `.hcert` artifacts rejected fail-closed**. |
