// ============================================================================
// control-composite-literal-in-range — Checker corpus (bounty track: "grow the adversarial corpus").
//
// PURPOSE: the in-range twin of the composite-literal overflow case (#519, fixed). The wave must reject `user{age: 300}` and accept `user{age: 127}`; this pins the accept half so the reject half cannot be widened into a blanket rule.
//
// RULE CITATION: Row 13 (BUILDABLE). 127 fits the int8 field; Go compiles it.
//
// ---- Why this is an ACCEPT (and must stay one) ----
// `u := user{age: 127}` and `xs := []int8{127}` are legal and must stay accepted. They sit one unit below the values #519 rejects, so an off-by-one in the composite-literal fix shows up here immediately.
//
// ---- Expected verdict (verified on main before this file was added) ----
// EXPECTED-VERDICT: ACCEPT
//   ACCEPT — 0 annotations, 0 managed. `hyb check` accepts it AND the emitted
//   Go compiles; both were run before this fixture was committed.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

struct user {
    age int8
}

fn main() {
    u := user{age: 127}
    xs := []int8{127}
    fmt.println("{u.age} {xs[0]}")
}
