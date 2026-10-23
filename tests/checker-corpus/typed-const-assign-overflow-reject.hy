// ============================================================================
// typed-const-assign-overflow-reject — Checker corpus (issue #998).
//
// PURPOSE: a NAMED file-level constant assigned into a narrow-typed variable
// through the ASSIGNMENT target (`x = k`), not a declaration initializer.
//
// RULE CITATION: GUARANTEES.md Row 13 (BUILDABLE) — every `hyb check`-accepted
// file must build cleanly with `go build`. Go folds the constant and refuses
// the emitted assignment:
//   cannot use k (untyped int constant 70000) as int16 value in assignment (overflows)
//
// ---- Why this is a REJECT ----
// The assign-side overflow check existed (wired through `place_target_ty` in
// loans.rs and emit_guard.rs) but folded the RHS with a bare syntactic fold
// that saw the identifier `k` as NotConst — so the literal form (`n = 300`)
// rejected while the named-const twin (`x = k`) passed with 0 diagnostics.
// The declaration path resolved named consts through the file const map
// (W10: `const big = 300; var n int8 = big`); the assign path now shares that
// fold, so both twins reject identically.
//
// Found by the W-023 Go-toolchain oracle (wildcard #565, spike #577),
// INVARIANT-4, case 30/66 (`const_overflow` family); re-verified on main
// d7ab63a and ec3fe0b on 2026-10-20/21.
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: REJECT BUILDABLE
// EXPECTED-DIAGNOSTIC: HYB-E6002
//
// ---- Paired controls (must stay clean) ----
// The two in-range lines below are the local controls: `x = 5` fits int16,
// and `y = k` fits int32 — only the `x = k` line fires. The sibling
// `typed-const-expression-controls-accept.hy` pins the run-time wrapping
// fence (SPEC X1/X2): non-constant operands stay out of scope.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

const k = 70000

fn main() {
    var x int16 = 1
    x = 5                           // in-range literal: stays clean
    x = k                           // 70000 does not fit int16: HYB-E6002
    var y int32 = 1
    y = k                           // fits int32: stays clean
    _ = x
    _ = y
}
