# W-004 Spike Design: Zero-Checker Emit via CPS-Scoped Phantom Witnesses

**Track:** Wildcard Track ($500 pool)  
**Slot:** Slot 4 (2026-11-01 Review Window)  
**Issue Reference:** #174  
**Author:** area523ufo (Proposal) / @jpereyra243-byte (Executable Spike & Evaluation)  
**Settlement Wallet:** USDC (Polygon / Base) `0xC175c34c256598C7e1AD5B15503cFcED7Ed67e10`  
**Status:** Pre-registered before execution  

## 1. Sacred Cows Attacked
1. **Sacred Cow #1:** `hy-check` must prove ownership in Rust before `hy-emit` erases it.
2. **Sacred Cow #2:** `hy-emit` must emit flat, line-for-line Go statements (`stmt.rs`) with ownership erased.
3. **Sacred Cow #3:** The `.hy` / `.go` boundary requires hand-curated `foreign.rs` tables.

## 2. Core Architectural Hypothesis
Instead of maintaining a flat abstract interpreter in Rust (`walk.rs`, `loans.rs`, `freeze.rs`), `hy-emit` lowers ownership, freeze states, and loan windows directly into Go's static type system using zero-sized phantom capability tokens (`struct{}`) and continuation-scoped lexical shadowing:
- Non-sendable binding `x: T` is paired with a zero-byte capability token `hyCap_x: hyLive[R]`.
- Transfer sites (`task f(move x)` or `ch <- move x`) open a lexical continuation scope: `{ hyCap_x := hyMoved{}; ... }`.
- Every subsequent read/write emits a zero-cost witness check `hyUse(hyCap_x)` requiring `hyLive[R]`.
- Loop bodies end with a static backedge witness `var _ hyLive[R] = hyCap_x` before `}` and before every `continue`.
- `cmd/compile` itself enforces `HYB-E3003` (use-after-move), loop-backedge double-move, and parameter borrows with zero runtime overhead.

## 3. Pre-Registered Hypotheses & Falsifiers

### Hypothesis P1: Zero Runtime & Binary Overhead
- Zero-byte phantom structs (`struct{}`) and inlined witness helpers (`func hyUse[R any](hyLive[R]) {}`) are completely eliminated by `cmd/compile`.
- **Target:** 0 heap allocations, 0 bytes/op overhead in benchmarks, and clean escape analysis (`-gcflags=-m`).

### Hypothesis P2: Static Enforcement by `cmd/compile`
- Canonical Tier-0 violations fail at `go build` time directly in `cmd/compile`:
  - Straight-line use-after-move (`A1b`)
  - Loop-backedge double-move (`A1c`)
  - Moving borrowed parameters / receivers (`A5`)

### Falsifier P3: The Control-Flow Join Wall (Honest Boundary Report)
- **Hypothesis to Falsify:** Can pure lexical shadowing in Go handle conditional moves across control-flow joins without compiler AST support?
- **Expected Boundary:** A move inside `if cond { move x }` opens a lexical block that pops at `}`, losing the `hyMoved` witness after the join unless statements following the join are CPS-tail-duplicated or hoisted. Proving exactly where lexical shadowing succeeds vs. requires compiler support is the required M2 learning deliverable.
