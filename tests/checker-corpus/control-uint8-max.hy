// ============================================================================
// control-uint8-max — Checker corpus (bounty track: "grow the adversarial corpus").
//
// PURPOSE: the unsigned edge. `uint8` has no negative side, so a signed-only range check either skips unsigned types entirely or mishandles the 0 floor. 255 is the legal maximum and must be accepted.
//
// RULE CITATION: Row 13 (BUILDABLE). 255 fits uint8; Go compiles it.
//
// ---- Why this is an ACCEPT (and must stay one) ----
// `var n uint8 = 255` is legal and must stay accepted. A checker that treats unsigned types with the signed range, or that rejects the maximum because it compares against an exclusive bound, fails here.
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
    var n uint8 = 255
    fmt.println("{n}")
}
