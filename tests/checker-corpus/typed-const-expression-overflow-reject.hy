// ============================================================================
// typed-const-expression-overflow-reject — Checker corpus (issue #725).
//
// PURPOSE: a typed constant expression whose FOLDED RESULT overflows its type.
//
// RULE CITATION: GUARANTEES.md Row 13 (BUILDABLE) — every `hyb check`-accepted
// file must build cleanly with `go build`.
//
// ---- Why this is a REJECT ----
// Go folds constant expressions BEFORE the range check, so `u8(255) + 1` is
// `constant 256 of type uint8 overflows uint8`. `hyb check` accepted it with 0
// diagnostics on main @ aa4a9d9 and the emitted Go failed `go build`.
//
// The per-operand check could not see it: 255 fits `u8` and 1 fits `u8`, so
// both operands passed and the RESULT was never computed. Shifts took an early
// return and never reached any range check at all, and the unary arm had none.
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: REJECT BUILDABLE
// EXPECTED-DIAGNOSTIC: HYB-E6002
//
// ---- Paired fixture ----
// `typed-const-expression-controls-accept.hy` carries the accept half. The
// load-bearing control is run-time arithmetic on narrow types: SPEC X1/X2
// define that as WRAPPING, so `var x u8 = 200; x + 1` must keep building.
// Rejecting it would be the over-correction this pair exists to prevent.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

const top u8 = 255

fn main() {
    a := top + 1
    b := u8(250) + u8(10)
    c := u8(0) - 1
    d := u8(200) * 2
    e := u8(1) << 9
    f := -u8(1)
    g := i64(1) << 63
    h := i8(127) + i8(1)
    fmt.println("{a} {b} {c} {d} {e} {f} {g} {h}")
}
