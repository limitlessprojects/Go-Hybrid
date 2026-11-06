# W-024 Spike Design: In-Memory go/types Back-Projection as the Exact Row-13 Decider

**Track:** Wildcard Track ($500 pool)  
**Issue Reference:** #3674  
**Author:** @jpereyra243-byte (Settlement: `0xC175c34c256598C7e1AD5B15503cFcED7Ed67e10`)  
**Status:** Pre-registered before execution  

## 1. Sacred Cow Attacked
Deleting `buildable.rs` (~214 KB / 4,945 LOC) and `emit_guard.rs` (~116 KB / 2,649 LOC) along with their ~180 KB of shadow typing tables and regression harnesses (~510 KB total).  
The compiler ceases to maintain a hand-written partial re-implementation of Go's type-checker in Rust.

## 2. Core Architectural Hypothesis
Instead of approximating Go's type system in `hy-check`, `hy-check` verifies *only* Tier-0 ownership and sendability invariants. Buildability and type validity (`GUARANTEES.md` Row 13 `BUILDABLE` / `INVARIANT 4`) are decided by running Go's official `go/types` in-memory over span-injected emitted Go code, with errors back-projected to original `.hy` file spans.

## 3. Pipeline Architecture
```
.hy Source
   │
   ▼
hy-syntax (AST with exact Spans)
   │
   ▼
hy-check (Pure Tier-0: Sendability, Loans, Moves, Freezes)
   │
   ▼
hy-emit (Span-Injected Go source with //line directives)
   │
   ▼
[In-Memory go/types Decider]  <── Cached stdlib export data (types.Importer)
   │
   ├── Valid?  ──> emit canonical .hy.go / compile binary
   │
   └── Errors? ──> Back-project Pos -> Span, map error message -> HYB-E1002/E2006
```

## 4. Diagnostic Code Mapping Contract
| Go Error Class / Message Signature | Hybrid Diagnostic Code | Rule Name / Citation |
|---|---|---|
| `operator == not defined`, `cannot compare`, `only be compared to nil` | `HYB-E2006` | Incomparable equality (`emit_guard.rs`) |
| `negative shift count` | `HYB-E6005` | Constant shift bounds (`emit_guard.rs`) |
| `invalid array length`, `array length must be non-negative` | `HYB-E6004` | Array length validity (`emit_guard.rs`) |
| `... redeclared in this block` | `HYB-E1003` | Redeclaration (`hy-diag`) |
| `not enough arguments`, `too many arguments` | `HYB-E1002` | Call arity mismatch C9 (`buildable.rs`) |
| `cannot use ... in return statement`, `wrong number of return values` | `HYB-E1002` | Return signature mismatch C10 (`buildable.rs`) |
| `declared and not used`, `no new variables on left side of :=` | `HYB-E1002` | Unused / invalid := binding (`buildable.rs`) |
| `operator ... not defined on ...`, `mismatched types` | `HYB-E1002` | Binary operator operand mismatch (`buildable.rs`) |
| `cannot index ...`, `cannot slice ...` | `HYB-E1002` | Non-indexable / non-sliceable base (`buildable.rs`) |

## 5. Pre-Registered Falsifiers

### Falsifier F1: Span and Diagnostic Attribution Fidelity
- **Condition:** Must achieve 100% exact 1-based (line, column) span attribution and matching diagnostic code across all canonical Row-13 reject fixtures from `tests/checker-corpus/`.
- **Falsified if:**
  1. `//line` directives lose column fidelity or misattribute errors to surrounding synthetic wrappers (e.g. option prelude, exit wrapper, try-hoist temps).
  2. Any canonical Row-13 error cannot be classified into the expected `HYB-E1002`, `HYB-E2006`, `HYB-E1003`, `HYB-E6004`, or `HYB-E6005` code.

### Falsifier F2: In-Memory `go/types.Check` Latency Budget
- **Budget:** Steady-state warm check latency must be **< 1.2 ms per file** on typical source files (50–300 LOC) when using cached stdlib export data.
- **Falsified if:** Median warm check latency $\ge 1.2\text{ ms/file}$.
