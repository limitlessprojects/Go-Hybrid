# W-031: The Rule-Mutation Score — Measuring Corpus Detection Power

**Issue:** [#2198](https://github.com/limitlessprojects/Go-Hybrid/issues/2198)  
**Author:** `@Dimany4`  
**Ledger Slot:** `W-031` (Window 2026-11-01)  
**Milestone:** M1 Accepted Into Ledger  

---

## 1. Executive Summary & Problem Formulation

In Go-Hybrid, `differential_loop.py` prints:
> `accept 41/41 · reject 86/86 (FULL AGREEMENT)`

While this provides evidence that the checker matches existing fixtures, it has never measured whether the corpus would **notice** if the checker stopped checking. A green test run only proves agreement with hand-written tests; it proves nothing about whether those tests would detect a weakened or disabled rule.

Proposal **W-031** establishes the **Rule-Mutation Score**:
1. For each diagnostic rule ID the checker can report (`HYB-E3001` to `HYB-E3020`, `HYB-E4001`, etc.), systematic mechanical mutants are applied to the checking logic:
   - `OP_DELETE`: completely remove the check (always accept).
   - `OP_INVERT`: negate the predicate (reject valid, accept invalid).
   - `OP_WIDEN`: widen the accepted set (weaken fail-closed path).
   - `OP_SWAP`: emit a generic error instead of the pinned diagnostic code.
2. The corpus is run unchanged against each mutant.
3. A mutant is **KILLED** if the corpus goes RED (detects the flaw). A mutant **SURVIVES** if the corpus still reports `FULL AGREEMENT`.
4. The **Rule-Mutation Score** ($\frac{\text{Killed}}{\text{Total}} \times 100\%$) becomes the formal gate for `GUARANTEES.md`: no guarantee row may stand without a killed mutant proving the rule is defended by real fixtures.

---

## 2. 10-Angle Deep Invariant & Cybersecurity Analysis

| Angle | Hazard / Pitfall | Formal Resolution / Invariant |
|---|---|---|
| **1. Equivalent Mutants (Dead Code)** | Mutating unreachable or dead code falsely reporting unkilled mutant. | Mutator requires mutants to be *reportable*: must reside on active diagnostic emission paths with verifiable execution counts. |
| **2. Pinned Diagnostic Code Sensitivity** | Checker emits generic error instead of pinned code, but test passes. | Corpus harness enforces exact diagnostic code matching (`EXPECTED-DIAGNOSTIC`). Code swapping immediately kills mutant. |
| **3. CI Execution Budget (< 60s)** | Recompiling Rust compiler 20+ times taking 30 minutes in CI. | Fast in-process interceptor toggles rule logic dynamically without binary recompilation, running 20 rules in $< 1$s. |
| **4. Flaky Test False Positives** | Mutants killed due to timing or race detector jitter. | Hermetic evaluation: each test is evaluated deterministically in-memory against deterministic AST checks. |
| **5. Gap Reporting for New Fixtures** | Unkilled mutant found, but developer does not know what fixture to write. | Engine automatically outputs recommended fixture templates naming the exact missing rule. |
| **6. Fail-Closed Path Coverage** | Mutants weakening `HYB-E3005` (unproven names) slipping past. | Explicit `OP_WIDEN` operator verifies that fail-closed fallbacks are actively pinned by adversarial fixtures. |
| **7. Multi-Rule Interference** | One mutant triggering cascading false errors in unrelated checks. | Mutations are isolated per rule evaluation pass; cross-rule leakage is detected and quarantined. |
| **8. Mutation Taxonomy Completeness** | Missing mutation classes (e.g. partial predicates). | 4 distinct mutation operators cover deletion, negation, widening, and diagnostic code mutation. |
| **9. Quantitative Threshold Gate** | Arbitrary passing criteria. | Gate requires 100% of P0 safety rules and $\ge 90\%$ overall mutation kill rate to certify `GUARANTEES.md`. |
| **10. Demotion Gate Integration** | Integration with W-026 Demotion Gate. | Rules with surviving mutants trigger automatic demotion tickets in `MOONSHOT.md`. |

---

## 3. Operational Semantics & Mutation Scoring

Let $\mathcal{R} = \{r_1, r_2, \dots, r_N\}$ be the registered diagnostic rules, and $\mathcal{M}(r)$ be the set of generated mutants for rule $r$.

$$\text{Verdict}(\mathcal{C}, m) = \begin{cases} \text{Killed}, & \text{if } \exists t \in \mathcal{C} : \text{Run}(m, t) \neq \text{Expected}(t) \\ \text{Survived}, & \text{otherwise} \end{cases}$$

$$\text{Score}(\mathcal{R}) = \frac{\sum_{r \in \mathcal{R}} |\{m \in \mathcal{M}(r) : \text{Verdict}(\mathcal{C}, m) = \text{Killed}\}|}{\sum_{r \in \mathcal{R}} |\mathcal{M}(r)|} \times 100\%$$

### Guarantee Gate Invariant
$$\forall \text{ row } k \in \text{GUARANTEES.md} : \text{Score}(\mathcal{R}_k) = 100\% \quad (\text{Gate-Row-Proof})$$

---

## 4. Falsification Protocol & M1 Deliverables

- **M1 Deliverables (This Spike)**:
  - Reference mutation engine and gate verifier `mutation_score_gate.py`.
  - Automated test suite `test_mutation_score.py` proving mutant generation, kill detection, surviving mutant gap reporting, and score gating.
  - Registration in `MOONSHOT.md` as slot `W-031`.
- **M2 Falsifiers**:
  - If $\ge 18$ of 20 hand-written mutants are killed by the existing corpus without any additions, the author's premise is wrong (corpus is already strong), and that becomes the paid M2 learning.
  - If mutants survive, the deliverable is one new fixture per surviving mutant until the score reaches 100%.
