# W-024: Certified Heap-Freedom (`no_heap`) via Tier-0 Escape Proofs + Stack-Array Lowering

**Track:** Moonshot Wildcard Track (Prompt #4 — Static Heap-Freedom Proofs on Safe Go)  
**Issue:** [#1172](https://github.com/limitlessprojects/Go-Hybrid/issues/1172)  
**Status:** Pre-registered Executable Spike  
**Author:** umpireboulianne-design / Dimank92  
**Target Payout Wallet:** `0xC345C07229307d4f1c7935aD2cAe48e390215808` (Base / Polygon USDC)  

---

## 1. Breakthrough Hypothesis
Turn Tier-0's existing ownership, loan-window (`A1`–`A6`), and boundary-crossing (`task` / `chan`) analysis into a **compile-time Heap-Freedom Certificate (`no_heap`)** paired with **allocation-directed Go emission**, verified against `go build -gcflags="-m=2"` as a deterministic oracle.

Today, `hy-check` computes a whole-function alias (`may_share`), loan, and boundary-escape lattice that is strictly stronger than what `hy-emit` uses: `hy-emit` discards those proofs and emits naive Go (`make([]T, n)`, unspecialized closures, boxed interface calls) that `cmd/compile`'s heuristic escape analyzer frequently spills to the GC heap.

By:
1. Extending Tier-0 with a one-pass non-escape certificate over `FnEnv` (rejecting any value in a `no_heap` fn that flows to a `task` spawn, `chan` send, `return` of reference shape, foreign A5 unknown perimeter, or dynamic-growth collection).
2. Teaching `hy-emit` to lower Tier-0 proven frame-bounded slices (`make([]T, n, K)` with constant `K`) and non-escaping closures into stack-allocated backing arrays (`var _hyStack [K]T; s := _hyStack[:n:K]`) and unboxed calls.
3. Gating `hyb build` on `go build -gcflags="-m=2"` reporting zero `escapes to heap` / `moved to heap:` diagnostics on certified functions.

Hybrid gains a provable, compiler-verified **Heap-Freedom Guarantee** on ordinary Go without touching the Go runtime or using `unsafe`.

---

## 2. Three Concrete Falsifiers

### Falsifier 1: Zero False-Accept Soundness against `cmd/compile -m=2` (P1)
- **Claim:** Every function marked with `//hy:no_heap` that passes Tier-0 non-escape checking produces zero `escapes to heap` and zero `moved to heap:` compiler diagnostics in `go build -gcflags="-m=2"`.
- **Falsification Condition:** Any accepted `no_heap` function triggering a heap diagnostic or exhibiting `testing.AllocsPerRun > 0` at runtime immediately falsifies the certificate.

### Falsifier 2: Emitter Stack-Promotion Delta on Conservative Spills (P2)
- **Claim:** Frame-bounded slice scratch allocations (`make([]byte, len, 1024)`) lowered to stack backing arrays reduce heap allocations from $\ge 1$ allocs/op to strictly $0$ allocs/op across standard Go compilers (Go 1.22+).
- **Falsification Condition:** If `cmd/compile` spills the backing array to heap under practical buffer limits ($K \le 4096$ bytes), P2 fails.

### Falsifier 3: Foreign Perimeter Boundary Precision (P3)
- **Claim:** Calls to non-allocating Go standard library primitives (e.g. `binary.LittleEndian`, `strings.Index`, `math.Min`) can be proven non-escaping without requiring whole-program Go source analysis via curated descriptors in `shapes.rs`.
- **Falsification Condition:** If calling basic non-allocating primitives cannot be decoupled from interface boxing allocations without $> 100$ LOC of hardcoded descriptor stubs, P3 fails.

---

## 3. Spike Test Suite
The spike contains:
- `falsifier_p1_test.go`: Validates zero runtime allocations (`testing.AllocsPerRun == 0`) on stack-lowered buffers.
- `falsifier_p2_oracle.py`: Parses `cmd/compile -m=2` escape analysis output and verifies clean stack placement.
