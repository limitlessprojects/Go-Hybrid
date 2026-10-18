// ============================================================================
// field-assign-wrong-type-reject — Checker corpus (bounty track: "grow the adversarial corpus").
//
// PURPOSE: assigning a value of the wrong type to a struct field.
//
// RULE CITATION: GUARANTEES.md Row 13 (BUILDABLE) — every `hyb check`-accepted
// file must build cleanly with `go build`.
//
// ---- Why this is a REJECT ----
// `q.a` is declared `int`, so `q.a = "str"` is `cannot use "str" (untyped
// string constant) as int value in assignment`. The checker previously accepted
// the file with 0 diagnostics.
//
// Closed by the #682 U16 rule (HYB-E1002,
// `buildable.rs::check_assign_operands`). The check runs only for the
// single-target/single-value form; a multi-value right-hand side is left to the
// call-shape rules.
//
// ---- Expected verdict (reproduced as a real `go build` failure first) ----
// EXPECTED-VERDICT: REJECT BUILDABLE
// EXPECTED-DIAGNOSTIC: HYB-E1002
//
// ---- Paired fixture ----
// `struct-and-conversion-controls-accept.hy` holds the legal neighbours, so
// these rules cannot be widened into something that refuses buildable programs.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

struct s {
    a int
}

fn main() {
    var q s
    q.a = "str"
    fmt.println("{q.a}")
}
