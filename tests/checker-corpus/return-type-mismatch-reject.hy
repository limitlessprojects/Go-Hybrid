// ============================================================================
// return-type-mismatch-reject — Checker corpus (bounty track: "grow the
// adversarial corpus").
//
// PURPOSE: a `return` whose value cannot be assigned to the declared result
// type. The emitted Go cannot build, so accepting it breaks GUARANTEES.md
// Row 13 (BUILDABLE).
//
// RULE CITATION: Row 13. Go refuses the statement:
//   `cannot use "not-an-int" (untyped string constant) as int value in
//    return statement`
//
// ---- Why this is a REJECT ----
// The result is `int` and the returned value is a string literal. Go has no
// implicit conversion between the string and numeric families, so the emitted
// `return "not-an-int"` never builds. The checker previously accepted the file
// with 0 diagnostics. Closed by the #577 C10 kind rule (HYB-E1002,
// `buildable.rs::kinds_conflict` / `check_return`).
//
// The container forms are covered by the same rule and by
// `compiler/crates/hy-check/tests/call_arity_and_return_types.rs`: a
// `[]string` returned from a `[]int` function, a map where a slice is
// declared, and a different struct type.
//
// ---- Expected verdict (reproduced as a real `go build` failure first) ----
// EXPECTED-VERDICT: REJECT BUILDABLE
// EXPECTED-DIAGNOSTIC: HYB-E1002
//
// ---- Paired control ----
// `call-and-return-controls-accept.hy` holds the assignable neighbours: an
// untyped int constant returned from a `float64` function, an `int8` result
// with an in-range constant, a matching container shape, and an interface
// result. The rule must stay fail-open on anything it cannot prove — this is
// an accept-side check, and an over-reject is a regression.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

fn bad() int {
    return "not-an-int"
}

fn main() {
    x := bad()
    fmt.println("{x}")
}
