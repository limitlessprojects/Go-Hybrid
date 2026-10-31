# W-033: Inline Ghost-Capability Directives — Erasing the `.hy` File Split via Standard Go AST Comment Pragmas

**Issue:** [#2406](https://github.com/limitlessprojects/Go-Hybrid/issues/2406)  
**Author:** `@zhangbailu168-wq`  
**Ledger Slot:** `W-033` (Window 2026-11-01)  
**Solicitation Answered:** `#1 Kill the .hy / .go split.`  
**Milestone:** M1 Accepted Into Ledger  

---

## 1. Executive Summary & Problem Formulation

The current Hybrid architecture requires a syntactic bifurcation: developers must author `.hy` files that compile into Go, creating friction with IDEs, `gopls`, and the broader Go ecosystem. Proposal **W-033** demonstrates that this split is unnecessary.

Instead of transpiling an external dialect, `hyb check` operates directly on standard Go AST (`go/parser`, `go/types`), treating compiler-friendly comment pragmas (`//hy:move`, `//hy:shared`, `//hy:task`, `//hy:region[R]`) as a **Ghost Capability Layer** that attaches static affine tokens to variables and expressions.

At compile time:
- The standard Go toolchain (`go build`, `gopls`, `go vet`) sees 100% pure idiomatic Go.
- `hyb check` verifies Hybrid's ownership, borrowing, and task-spawn contracts with mathematical rigor.
- The verification layer is completely phantom/ghost at runtime with **zero runtime overhead**.

---

## 2. 10-Angle Deep Architectural Analysis & Invariants

| Angle | Hazard / Pitfall | Formal Resolution / Invariant |
|---|---|---|
| **1. Tooling Parity & Zero AST Fork** | Bifurcated grammar breaks `gopls` and code navigators. | Direct parsing via `go/parser` and comment attachment (`ast.CommentGroup`). Zero IDE forks or custom LSP required. |
| **2. Use-After-Move Detection** | Value read after ownership transfer. | `//hy:move <var>` destructively consumes the affine token $\kappa(\text{var}) \mapsto \text{Moved}$. Subsequent accesses rejected with `HYB-E1001`. |
| **3. Concurrent Task Isolation** | Data races across goroutine boundaries. | `go func()` or `//hy:task` requires all captured variables to be explicitly moved (`//hy:move`) or shared (`//hy:shared`). Unannotated captures rejected with `HYB-E3001`. |
| **4. Region Containment & Escapes** | Pointers escaping off-heap or task-local arenas. | `//hy:region[R]` tags allocations with lifetime token $\rho_R$. Escaping references beyond region scope rejected with `HYB-E4001`. |
| **5. Incremental Package Adoption** | Requiring all-or-nothing project migration. | Opt-in per package via `//hy:check` header. Unannotated packages compile normally without checking overhead. |
| **6. Syntactic String Immunity** | Pragma text appearing inside string literals or logs. | Lexer sanitization masks all `"..."` and `` `...` `` literals before pragma extraction. |
| **7. Multi-Statement & Inline Attachment** | Pragmas on chained or semicolon-separated statements. | Pragma binding associates strictly with the immediate next AST statement node or target identifier. |
| **8. Transitive Alias Propagation** | Aliasing moved variables through pointer copies. | Alias graph maps canonical capability tokens across assignments (`dst := src`). |
| **9. Borrow Checker Lifetime Equivalence** | Mutable borrow escaping active loan window. | `//hy:borrow <var>` freezes base capability until borrow scope terminates. |
| **10. Falsifiability & Demotion Gate** | Protocol rotting or breaking standard Go idioms. | Connected to Automated Demotion Gate: fails M2 if annotating idiomatic Go introduces $> 0$ runtime regressions. |

---

## 3. Operational Semantics & Ghost Capability Rules

Let $\Gamma$ be the typing environment mapping variables to capability states $\kappa \in \{\text{Owned}, \text{Borrowed}, \text{Shared}, \text{Moved}\}$.

$$\frac{\Gamma(x) = \text{Owned} \quad \text{Stmt} \text{ annotated with } //\text{hy:move } x}{\Gamma \vdash \text{Stmt} \implies \Gamma[x \mapsto \text{Moved}] \quad (\text{T-Move})}$$

$$\frac{\Gamma(x) = \text{Moved} \quad \text{Read}(x)}{\Gamma \vdash \text{Read}(x) \implies \text{Error}(\text{HYB-E1001: Use of moved value}) \quad (\text{E-UseAfterMove})}$$

$$\frac{\text{TaskSpawn} \quad \text{Captures}(x) \quad \Gamma(x) \notin \{\text{Shared}, \text{Moved}\}}{\Gamma \vdash \text{TaskSpawn} \implies \text{Error}(\text{HYB-E3001: Concurrent data race hazard}) \quad (\text{E-DataRace})}$$

$$\frac{\text{Alloc}(x) \in \text{Region}(R) \quad \text{Return}(x)}{\Gamma \vdash \text{Return}(x) \implies \text{Error}(\text{HYB-E4001: Region escape}) \quad (\text{E-RegionEscape})}$$

---

## 4. Test Matrix & Falsification Criteria

### Negative Tests (Must Reject with Compile Error)
1. `neg_use_after_move.go`: Accessing variable after `//hy:move`.
2. `neg_concurrent_race.go`: Spawning goroutine capturing mutable variable without `//hy:move` or `//hy:shared`.
3. `neg_region_escape.go`: Returning pointer allocated within `//hy:region[R]` beyond its lifetime.

### Positive Test (Must Compile Cleanly)
4. `pos_pure_go_worker.go`: Standard Go pipeline with `//hy:check`, `//hy:move`, and `//hy:task` validating 100% green under both `go build` and `hyb check`.

---

## 5. Milestone Contract & Falsification Criteria
- **M1 Deliverables (This Spike)**:
  - Reference gate verifier `ghost_capability_gate.py`.
  - Comprehensive automated test suite `test_ghost_capabilities.py` proving all negative rejects and positive accepts.
  - Registration in `MOONSHOT.md` as slot `W-033`.
- **M2 Falsifier**:
  - If parsing comment pragmas over standard Go AST fails to capture the exact same soundness bugs as the bespoke `.hy` frontend across the corpus, proposal is falsified.
