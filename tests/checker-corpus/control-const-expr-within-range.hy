// ============================================================================
// control-const-expr-within-range — Checker corpus (bounty track: "grow the adversarial corpus").
//
// PURPOSE: a CONSTANT EXPRESSION whose value fits, in the same position the const-overflow wave checks. The wave must fold the expression and test the RESULT, not reject the arithmetic itself.
//
// RULE CITATION: Row 13 (BUILDABLE). `100 + 27` folds to 127, which fits int8.
//
// ---- Why this is an ACCEPT (and must stay one) ----
// `var n int8 = 100 + 27` is legal. A check that rejects a constant expression by comparing an intermediate value, or that rejects any arithmetic in an initializer, over-rejects here. The counterpart — `var n int8 = 127 + 1` — is a genuine violation (E6002) and is covered by the const-overflow wave, not by this control.
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
    var n int8 = 100 + 27
    fmt.println("{n}")
}
