// ============================================================================
// conversion-arity-reject — Checker corpus (bounty track: "grow the adversarial corpus").
//
// PURPOSE: a type conversion with more than one argument.
//
// RULE CITATION: GUARANTEES.md Row 13 (BUILDABLE) — every `hyb check`-accepted
// file must build cleanly with `go build`.
//
// ---- Why this is a REJECT ----
// `int(1, 2)` is not a conversion Go accepts. A conversion has the form `T(x)`
// — exactly one argument — so Go reports `too many arguments in conversion to
// int`. The checker previously accepted the file with 0 diagnostics.
//
// Closed by the #682 U13 rule (HYB-E1002,
// `buildable.rs::check_conversion_arity`). It fires only for a bare identifier
// that resolves to a KNOWN type, which is what keeps builtins (`len`, `append`,
// `make`, ...) on their own arity rules in `builtins.rs` and user functions on
// `check_user_fn_arity`.
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

fn main() {
    v := int(1, 2)
    fmt.println("{v}")
}
