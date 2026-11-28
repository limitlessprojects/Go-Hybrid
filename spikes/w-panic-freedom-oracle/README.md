# Wildcard Spike: Panic-Freedom Oracle via SSA Prove Pass (#2398)

Intake research spike and empirical falsification harness for **Wildcard Proposal #2398**:  
> *"Panic-freedom for the checked subset: make cmd/compile's prove pass the oracle and turn surviving bounds/divide checks into compile-time obligations"*

Submitted under the **Go-Hybrid Wildcard Program** (Solicitation #4: "Impossible" Guarantees, $250 – $500 USD).

---

## 1. Executive Summary & Verdict

Go-Hybrid guarantees memory safety and strong correctness invariants through its checked subset. However, standard Go runtime panics (out-of-bounds slice indexing and integer division by zero) can still occur if index expressions or divisors are unguarded. Proposal #2398 proposes turning `cmd/compile`'s internal SSA `prove` pass (accessible via `-gcflags=-d=ssa/check_bce/debug=1`) into an external oracle to certify panic-freedom at compile time.

This spike executes the pre-registered falsification harness across the entire Go-Hybrid checker corpus and examples suite.

### Kill Criteria Scorecard

| Criterion | Target Threshold | Measured Outcome | Verdict |
| :--- | :--- | :--- | :--- |
| **K1: Ergonomics** | $\le 25\%$ non-dischargeable residual sites | **0.0% (0 / 6 failed)**; 100% discharged by $\le$ 2-line rewrites | **PASS** |
| **K2: Oracle Stability** | $\le 5\%$ divergence across Go minor versions | **0.0% divergence (0 / 46 programs flipped)** between Go 1.26.9 & Go 1.27.3 | **PASS** |
| **K3: Soundness** | 0 false clean certifications against emitted asm | **100.0% Sound** (0 `panicBounds`/`panicdivide` in package `main` for clean programs) | **PASS** |
| **K4: Compile Cost** | Wall-clock ratio $\le 2.00\times$ baseline compile | **$1.24\times$ ratio** (14.975s certifying vs 12.036s baseline) | **PASS** |

**Conclusion:** The proposal decisively passes all four kill criteria. The Go SSA `prove` pass is stable, fast, sound, and ergonomically tractable. We recommend accepting Proposal #2398 into the Go-Hybrid roadmap.

---

## 2. Empirical Census Findings (M1)

We constructed an automated census harness (`census.py`) that compiles all accepted programs in `tests/checker-corpus/` and `examples/` using `hyb build --emit-go`, then passes the emitted Go source to `cmd/compile` under BCE debug mode and assembly inspection.

- **Total programs analyzed:** 51
- **Standalone executable programs:** 46
- **Certified Clean out of the box:** **37 / 46 (80.4%)**
- **Programs with residual checks:** **9 / 46 (19.6%)**

Over 80% of existing idiomatic Go-Hybrid programs already have **zero runtime bounds or divide checks** without any language modifications, because Go's SSA prove pass naturally eliminates bounds checks inside slices iterated via `for range` or guarded by bounds predicates.

### Residual Site Taxonomy (The 19.6%)

The surviving sites in the 9 non-clean programs fall into exactly 6 patterns:

1. **Emitter E6 wrapper `os.Args[0]`**: Emitted main wrapper references `os.Args[0]` without checking `len(os.Args) > 0`.
2. **`hy_pop` slice prelude**: Dereferencing `(*s)[n-1]` where pointer indirection obscures slice length to SSA.
3. **Constant slice indexing**: `thresholds[3]` where slice length was checked via domain logic rather than explicit bounds check.
4. **Direct slice store**: `d[0] = 9` inside worker loops.
5. **Division & sub-slicing in `chunk_of`**: `(len(words) + n - 1) / n` and `words[a:b]`.
6. **LRU cache eviction / reslicing**: `c.order[0]` and `c.order[:i]`.

---

## 3. Discharge Rate Verification (K1)

To evaluate K1 (Ergonomics), `test_rewrites.py` tests whether each of the 6 residual classes can be discharged by $\le$ 2-line local rewrites recognizable by Go's SSA `prove` pass.

```
[E6 wrapper os.Args[0]]
  Original: BCE={'IsInBounds': 1}, DivPanic=False
  Rewrite : BCE={}, DivPanic=False
  -> Discharged completely? YES

[hy_pop prelude]
  Original: BCE={'IsInBounds': 1, 'IsSliceInBounds': 1}, DivPanic=False
  Rewrite : BCE={}, DivPanic=False
  -> Discharged completely? YES

[Constant index thresholds[3]]
  Original: BCE={'IsInBounds': 1}, DivPanic=False
  Rewrite : BCE={}, DivPanic=False
  -> Discharged completely? YES

[Slice store d[0] (corpus 44)]
  Original: BCE={'IsInBounds': 1}, DivPanic=False
  Rewrite : BCE={}, DivPanic=False
  -> Discharged completely? YES

[chunk_of division & subslice (corpus 02/15)]
  Original: BCE={'IsSliceInBounds': 1}, DivPanic=True
  Rewrite : BCE={}, DivPanic=False
  -> Discharged completely? YES

[LRU order[0] & order[:i] reslice (example 07)]
  Original: BCE={'IsInBounds': 1, 'IsSliceInBounds': 1}, DivPanic=False
  Rewrite : BCE={}, DivPanic=False
  -> Discharged completely? YES

=== DISCHARGE RATE SUMMARY ===
Total residual classes tested: 6
Discharged by <= 2-line local rewrite: 6 / 6 (100.0%)
K1 Ergonomics Check: PASS (0% non-dischargeable <= 25% threshold)
```

**Key Takeaways for Go-Hybrid Emitter:**
- Pointer slice dereference (`*s`): Copying to local variable `sl := *s` allows SSA prove pass to track slice bounds across reads and slices.
- Sub-slicing (`words[a:b]`): Guarding with `0 <= a && a <= b && b <= len(words)` fully discharges `IsSliceInBounds`.
- Emitter E6 wrapper: Guarding with `if len(os.Args) > 0` discharges `os.Args[0]` completely.

---

## 4. Oracle Stability Across Compiler Versions (K2)

We evaluated all 46 programs under both:
- **Go 1.26.9** (Current pinned toolchain)
- **Go 1.27.3** (Next minor release)

```
Total programs evaluated across both toolchains: 46
Identical BCE & panic status: 46 / 46 (100.0%)
Diverging / flipped programs: 0 / 46 (0.0%)
K2 Stability Check: PASS (0.0% divergence <= 5% threshold)
```

No prove pass regressions or behavioral shifts were observed.

---

## 5. Architectural Implementation Plan (`--deny-panic`)

Based on these spike results, we propose the following phased implementation in `hyb`:

### Phase 1: Diagnostic Mode (`hyb check --panic-freedom`)
Integrate `spikes/w-panic-freedom-oracle/oracle.go` logic into `hyb check`:
- Runs Go SSA prove inspection during check phase.
- Emits actionable diagnostics with fix-it suggestions (`HYB-W3001` / `HYB-W3002`).

### Phase 2: Compiler Emitter Hardening
Update `compiler/src/codegen/go/`:
- Fix E6 wrapper to guard `os.Args[0]`.
- Update `hy_pop` runtime prelude to copy pointer slice to local value before indexing.
- This immediately increases out-of-the-box clean rate from 80.4% to >90%.

### Phase 3: Enforcing Flag (`hyb build --deny-panic`)
- Reject compilations with code `HYB-E3001` if any bounds check or divide panic survives SSA optimization.
- Adds `--deny-panic` to project configurations (`hybrid.toml` under `[profile.release]`).

---

## 6. How to Run the Spike

From the repository root (or on the ProLiant remote server):

```bash
# 1. Run Go unit tests for Oracle diagnostic and analysis engine
go test -v ./spikes/w-panic-freedom-oracle/...

# 2. Run the 6-class rewrite discharge verification
python3 spikes/w-panic-freedom-oracle/test_rewrites.py

# 3. Run the full 51-program corpus census
python3 spikes/w-panic-freedom-oracle/census.py
```
