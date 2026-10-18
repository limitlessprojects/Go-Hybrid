// ============================================================================
// control-negate-variable-wraps — Checker corpus (bounty track: "grow the adversarial corpus").
//
// PURPOSE: negation of a VARIABLE, not a constant. Constant folding sees the value and may be tempted to range-check the result; Go does not fold a variable's negation, so it wraps at run time and compiles.
//
// RULE CITATION: Row 13 (BUILDABLE). `-n` where `n` is an int8 variable is a run-time operation in Go; the compiler does not evaluate it.
//
// ---- Why this is an ACCEPT (and must stay one) ----
// `var n int8 = -128; var m int8 = -n` compiles in Go and yields -128 (two's-complement wrap). A checker that folds the negation and rejects `-(-128)` as out of range would be reasoning about a value Go never computes at compile time. The CONSTANT twin `var m int8 = -(-128)` is a different question and belongs to the overflow wave.
//
// ---- Expected verdict (verified on main before this file was added) ----
// EXPECTED-VERDICT: ACCEPT
//   ACCEPT — 0 annotations, 0 managed. `hyb check` accepts it AND the emitted
//   Go compiles; both were run before this fixture was committed.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

fn main() {
    var n int8 = -128
    var m int8 = -n            // run-time negation: wraps, does not overflow
    fmt.println("{m}")
}
