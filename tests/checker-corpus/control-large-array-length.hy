// ============================================================================
// control-large-array-length — Checker corpus (bounty track: "grow the adversarial corpus").
//
// PURPOSE: a large but legal ARRAY LENGTH. Array lengths are compile-time constants and are exactly the sort of value a range check might wrongly bound.
//
// RULE CITATION: Row 13 (BUILDABLE). `[300]int8` is a legal Go array type.
//
// ---- Why this is an ACCEPT (and must stay one) ----
// `var a [300]int8` is 300 bytes of legal Go. A fix that bounds array LENGTHS by the element type's range (rather than bounding the INDEX against the length) rejects this. The length is a count, not a value of the element type.
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
    var a [300]int8
    fmt.println("{len(a)}")
}
