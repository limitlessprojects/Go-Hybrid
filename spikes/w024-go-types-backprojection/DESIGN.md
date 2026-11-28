# W-024 Author Spike Design: Span-Injected Emit + In-Memory `go/types` Back-Projection as the Exact Row-13 Decider

- **Track:** Wildcard Track ($500 pool: $250 M1 acceptance + $250 M2 published learning)
- **Proposal Issue:** #3674 (Original Author: `@Shokun123`)
- **Author Settlement Wallet:** `0xc1184f2c85d3158c14dc8f10d9727128ad5fb9f3` (USDC on Base / BNB Smart Chain BEP20 / Ethereum / Polygon)
- **Status:** Pre-registered hypotheses F1 & F2 and complete executable verification harness

---

## 1. Sacred Cow Attacked

`hy-check` currently maintains two massive, hand-written partial re-implementations of Go's type-checker in Rust solely to satisfy `GUARANTEES.md` Row 13 (`BUILDABLE` / `INVARIANT 4`):
1. `crates/hy-check/src/buildable.rs` (~214 KB / 4,945 LOC)
2. `crates/hy-check/src/emit_guard.rs` (~116 KB / 2,649 LOC)
3. Shadow typing tables and ad-hoc Row-13 harnesses (~180 KB)

Because Go's specification has hundreds of contextual typing edges (untyped constant promotion, shift operand rules, recursive comparable struct/array fields, generic arity, composite literal elided keys, short-var redeclaration vs shadowing), re-implementing `go/types` in Rust produces an unending stream of soundness escapes (#3438, #3441, #3570, #3574, #3583, #3620, #3628, #3630, #3636, #3638, #3643, #3650).

## 2. Core Architectural Replacement

1. **Pure Tier-0 `hy-check`**: `hy-check` checks *only* Hybrid's Tier-0 ownership, sendability, capability, loan, and freeze invariants.
2. **Span-Injected `hy-emit`**: During emission, `hy-emit` emits canonical `/*line file.hy:line:col*/` inline directives (as well as line-anchored `//line` directives) preserving exact 1-based `(line, col)` spans across synthetic wrappers (`__hy_try`, `option` prelude, `defer` closures).
3. **In-Memory `go/types.Check` Decider**: The emitted buffer is validated in-memory against pre-warmed stdlib export data (`CachedImporter`), and `types.Error` / `scanner.ErrorList` positions are back-projected via `token.FileSet.Position(err.Pos)` directly into Hybrid's `Diagnostic` schema (`HYB-E1002`, `HYB-E1003`, `HYB-E2006`, `HYB-E6004`, `HYB-E6005`).

## 3. Pre-Registered Falsifiers

- **Falsifier F1 (Diagnostic Code & Span Attribution Fidelity):**
  - *Hypothesis:* Inline `/*line file.hy:L:C*/` and `//line` directives preserve 100% exact `(file, line, col)` attribution across all 10+ Row-13 error classes without synthetic wrapper bleed, and deterministic error classification maps every `go/types` diagnostic to its canonical `HYB-E*` code.
  - *Falsified if:* Any corpus reject fixture misattributes line/col to synthetic Go prelude lines or maps to the wrong `HYB-E*` code.
- **Falsifier F2 (Warm In-Memory Latency Budget):**
  - *Hypothesis:* Steady-state warm `go/types.Check` over cached stdlib `types.Package` objects completes in **< 1.2 ms/file** for typical 50-300 LOC `.hy` files.
  - *Falsified if:* Median warm latency $\ge 1.2\text{ ms/file}$.
