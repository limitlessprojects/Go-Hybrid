# W-030: Verdict-Differential Fuzzing — Turning the Corpus into a Generator (Not an Artifact)

**Issue:** [#2197](https://github.com/limitlessprojects/Go-Hybrid/issues/2197)  
**Author:** `@slabhousextracts-blip`  
**Ledger Slot:** `W-030` (Window 2026-11-01)  
**Milestone:** M1 Accepted Into Ledger  

---

## 1. Executive Summary & Problem Formulation

In Go-Hybrid, the 127 programs in `tests/checker-corpus/` serve as the specification. However, current verification tools only test these static points:
- `hyfuzz.py` mutates programs but checks only crash invariants (no hang, no crash, no ICE); it never verifies if the mutated program still receives the correct verdict.
- `corpus_harness.py` compares verdicts against `VERDICTS.md`, but only on unmutated, frozen files.

Proposal **W-030** unifies them into **Verdict-Differential Fuzzing**:
1. Seeds from the 127 corpus programs carrying registered verdicts (`EXPECTED-VERDICT: ACCEPT | REJECT(E...)`).
2. Applies semantics-preserving mutations (alpha-renaming, harmless wrappers, dead-code insertion, type annotation variation).
3. Discards any mutant that fails lexer/parser grammar checks (Parse-Clean Filter).
4. Runs the type-checker and borrow-checker: if the mutant's verdict differs from the parent's registered verdict, a **Verdict Flip (Candidate P0)** is declared.
5. Employs an automated Delta-Debugger ($DDmin$) to minimize the candidate case down to $< 40$ lines with 100% deterministic reproducibility from seed.
6. Converts the corpus from a static artifact into an automated generator of adversarial boundary cases.

---

## 2. 10-Angle Deep Invariant & Cybersecurity Analysis

| Angle | Hazard / Pitfall | Formal Resolution / Invariant |
|---|---|---|
| **1. False Positive Verdict Flips** | Mutations that legitimately alter program semantics (e.g. removing a move) declared as bugs. | Mutation operators strictly partitioned into **Equivalence-Preserving** ($M_1$–$M_4$) and **Boundary-Probing** ($M_5$). Flips in $M_1$–$M_4$ are 100% true P0 bugs. |
| **2. Parse-Clean Filter Throughput** | Subprocess invocation bottleneck on millions of malformed syntax mutants. | Fast in-process pre-parser validates grammar, brace balancing, and token streams before invoking compiler backend. |
| **3. Delta-Debugging Minimization** | Minimizer stuck in local minima or creating non-compiling fragments. | Hierarchical Delta-Debugging ($DDmin$) tests $1/2, 1/4, 1/8$ subdivisions with invariant preservation (must parse and must reproduce flip). |
| **4. Deterministic Replay Integrity** | Flaky fuzzer crashes unreproducible by maintainers. | Every candidate hit prints `--seed <uint64> --mutant <ID>`, enabling exact bitwise replay across environments. |
| **5. E30xx Family Coverage** | Mutator generating trivial variations that never reach ownership check-sites. | Mutator specifically targets `task`, `spawn`, `ch <- v`, `close()`, and `move` AST boundaries. |
| **6. Cross-Validation with Race Detector** | Fuzzer artifact vs real runtime data race. | Minimized ACCEPT candidates cross-checked against Go toolchain `go test -race` to prove soundness. |
| **7. Line Count Budget (< 40 lines)** | Minimizer delivering 200-line bloated bug reports. | $DDmin$ algorithm guaranteed to terminate with minimal reproducing example under 40 lines. |
| **8. Multi-Diagnostic Error Matching** | Parent expects `HYB-E3002`, mutant emits `HYB-E3005`. | Oracle matches exact diagnostic codes, flagging diagnostic drift as well as verdict flips. |
| **9. Seed Corpus Extensibility** | Tool hardcoded to existing 127 files. | Dynamic directory walker discovers all `.hy` corpus files and parses embedded `// EXPECTED-VERDICT` headers. |
| **10. Demotion Gate Integration** | Fuzzer becoming abandoned script. | Connected to W-026 Demotion Gate; nightly CI run required to maintain GUARANTEES row certifications. |

---

## 3. Operational Semantics & Mutation Operators

Let $\mathcal{P}$ be a seed program with expected verdict $\mathcal{V}(\mathcal{P}) \in \{\text{Accept}, \text{Reject}(C)\}$.

$$\frac{\mathcal{P}' = \mathcal{M}_{\text{equiv}}(\mathcal{P}) \quad \text{ParseClean}(\mathcal{P}')}{\mathcal{V}(\mathcal{P}') \stackrel{!}{=} \mathcal{V}(\mathcal{P}) \quad (\text{Soundness Oracle})}$$

$$\frac{\mathcal{V}(\mathcal{P}') \neq \mathcal{V}(\mathcal{P})}{\text{Candidate P0 Detected} \implies \text{DDmin}(\mathcal{P}') \to \mathcal{P}_{\text{min}} \quad (\text{E-Flip})}$$

Where mutation operators include:
- $M_1$ (Alpha-Renaming): $\sigma(x) \mapsto x'$ preserving lexical scopes.
- $M_2$ (Identity Insertion): Injecting `_ = 0`, `val = (val)`, redundant grouping.
- $M_3$ (Type Elision / Ascription): Swapping `var x int = 1` $\leftrightarrow$ `x := 1`.
- $M_4$ (Block Scoping): Wrapping non-escaping sub-expressions in neutral `{ ... }` blocks.

---

## 4. Falsification Protocol & M1 Deliverables

- **M1 Deliverables (This Spike)**:
  - Reference verdict-differential fuzzer `verdict_fuzzer.py` with seed parsing, mutation operators, and $DDmin$ delta-debugger.
  - Automated test suite `test_verdict_fuzzer.py` proving verdict preservation, flip detection, and $< 40$ line minimization.
  - Registration in `MOONSHOT.md` as slot `W-030`.
- **M2 Falsifiers**:
  - The track fails if 10k mutations produce zero flips *and* the mutation operators cannot be shown to reach the `E30xx` families at all.
  - The track fails if minimized hits exceed 40 lines.
