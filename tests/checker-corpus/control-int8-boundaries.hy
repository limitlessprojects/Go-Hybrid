// ============================================================================
// control-int8-boundaries — Checker corpus (bounty track: "grow the adversarial corpus").
//
// PURPOSE: the exact EDGES of the int8 range. The accept-side oracle work (the const-overflow wave) adds range checks to more contexts, and an off-by-one in any of them rejects the two values that are legal by definition. These are the boundaries a range check has to get right: 127 and -128 inclusive.
//
// RULE CITATION: GUARANTEES.md Row 13 (BUILDABLE): every `hyb check`-accepted file builds cleanly with `go build`. Both values fit int8, so Go compiles them and Hybrid must not reject them.
//
// ---- Why this is an ACCEPT (and must stay one) ----
// `var n int8 = 127` and `var n int8 = -128` are the maximum and minimum representable int8 values. A fix that rejects `> max` or `< min` correctly, but one that uses `>=` or `>` on the negative side rejects one of these. The pair is deliberately in one file so a regression cannot be fixed by accident for only one side.
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
    var hi int8 = 127
    var lo int8 = -128
    fmt.println("{hi} {lo}")
}
