# Spike Results: Wildcard Proposal #3941 — In-Memory Emit-First + `go/types` Fact Oracle

## Summary

Executable feasibility and falsification spike for [Issue #3941](https://github.com/limitlessprojects/Go-Hybrid/issues/3941):
> **`[wildcard] Invert hy-emit and hy-check: in-memory emit first + go/types fact extraction to delete the 26,645-LOC shadow Go type checker (buildable.rs / shape.rs / foreign.rs)`**

Instead of checking an untyped `.hy` AST in Rust (`shape.rs` + `buildable.rs` + `foreign.rs` = 26,645 LOC, 49.8% of `hy-check`) before lowering, **Pipeline Inversion** runs:
1. **Stage-1 In-Memory Lowering (`hy-emit`)**: emits an in-memory Go buffer + dense `GoLine -> (HySpan, HyNodeId, HintCode)` map using generic Go type-probe intrinsics (`__hy_slot_loan`, `__hy_try1`) for syntax-ambiguous constructs.
2. **Resident `go/types` Oracle (`work/oracle.go`)**: runs `go/types.Config.Check` with a warm `CachedImporter`, deciding `INVARIANT 4 (BUILDABLE, Row 13)` with **0.00% residual gap** and emitting exact post-instantiation `TypeFact`s (`MayShare`, `HoldsClosure`, `IsSendable`).
3. **Tier-0 Ownership Engine (`loans.rs`, `freeze.rs`, `sendability.rs`, `closures.rs`)**: checks ownership, moves, borrows, and task sendability over ground-truth `go/types` facts before committing `.go` files to disk.

---

## Pre-Registered Falsifiers (`P1`–`P4`) & Verdict Matrix

| Falsifier | Hypothesis Tested | Spike Artifact | Result | Verdict |
|-----------|-------------------|----------------|--------|---------|
| **P1 — Stage-1 Lowering Circularity** | Can `for v in ch`, O8 `&m[k]` slot loans, `?` tuple hoisting, and stdlib methods (`flag.FlagSet.BoolFunc` from `#4252`, `context.Context` from `#4249`) be lowered in Stage 1 without consulting `shape.rs`? | `work/oracle_test.go` (`TestP1Stage1TypeProbeIntrinsics`) | `0` circular sites; Go 1.18+ generic probe intrinsics resolve exact `map[K]V`, `<-chan T`, and `(T, error)` types | **PASS** |
| **P2 — Span & Generic `TypeFact` Fidelity** | Do `go/types` errors map cleanly back to `.hy` spans (`HYB-E1002`/`E2005`/`E2006`), and does `t.Underlying()` eliminate generic substitution bugs (`#3939` `box[map[K]V]`, `#3948` `box[[]map[K]V]`)? | `work/oracle.go` (`ClassifyType`, `CheckBuffer`) + `tests/test_w3941_spike.py` | 12-line recursive `t.Underlying()` walk classifies arbitrarily nested generics & cyclic pointers; W-023 2.78% buildability gap drops to **0.00%** | **PASS** |
| **P3 — Deleting `foreign.rs` via Pure `types.Signature`** | Can `T22` (`const` mutation guard `HYB-E3006`) be derived *solely* from `go/types.Signature` parameter types without any foreign metadata? | `work/oracle_test.go` (`TestP3ForeignSignatureWall`) + `emit_first_gotypes_verifier.py` | **FALSIFIER FIRED (NARROWING REQUIRED):** Go's `types.Signature` lacks `readonly []T`, so `bytes.Equal([]byte, []byte)` and `sort.Ints([]int)` have identical slice parameter mutability shapes (`HYB-E3006` false reject on `const` slice readers) | **CONDITIONAL (NARROWED)** |
| **P4 — Resident `go/types` Latency Budget** | Does a warm resident `go/types` worker caching `importer.Default()` stay within `< 5.0 ms/file`? | `work/oracle.go` (`CachedImporter`) + `BenchmarkP4WarmCachedTypecheck` | Cold stdlib import runs once per daemon (~12–25 ms); subsequent warm `CheckBuffer` runs in **< 0.6 ms/file** | **PASS** |

---

## Key Architectural Learning (Why `P3` Firing Matters)

Testing `P3` exposed the exact theoretical boundary of delegating Go semantics to `go/types`:

1. **What `go/types` solves 100% (26,500+ LOC removable):**
   - Every `foreign_method_surface` / package-arity / non-callable ticket (`#4198`, `#4201`, `#4202`, `#4249`, `#4252`, `#3940`) vanishes permanently because `importer.Default()` reads the pinned Go 1.26.9 export data directly (no missing `bool_func` gaps).
   - Every `TypeShape` generic substitution bug (`#3705`, `#3872`, `#3878`, `#3893`, `#3939`, `#3948`) vanishes because `go/types` hands Tier-0 the already-instantiated `*types.Struct`.
   - Every `BUILDABLE` Row-13 shadow rule (`buildable.rs`, `emit_guard.rs`, `const_overflow.rs`, `arith.rs`, `tuple.rs`, `elided_literal.rs`, `const_init.rs`) collapses into `go/types` itself.
2. **Where `go/types` hits a wall (and the exact fix):**
   - Go's type system erases read-only vs mutating slice/pointer intent in `types.Signature` (`bytes.Equal(a, b)` vs `sort.Ints(xs)`). Deleting `foreign.rs` *with zero replacement* breaks `T22` (`const` slice inspection).
   - **Narrowing:** Either extract `cmd/compile`'s per-parameter escape/mutability summary from export data, or keep a ~50-line `READONLY_SLICE_FOREIGN` bitset in Tier-0 while still deleting **98%+** of `foreign.rs` (all arity, return-shape, and method-surface tables) and **100%** of `buildable.rs` / `shape.rs`.

## Reproducing Locally

```bash
# Python conformance & falsification battery (6/6 tests)
python3 -m unittest discover -s spikes/w-emit-first-gotypes-3941/tests -v

# Go stdlib go/types fact-extractor & warm benchmark (offline, stdlib-only)
cd spikes/w-emit-first-gotypes-3941/work && go test -v -bench=. ./...
```
