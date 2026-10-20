// ============================================================================
// untyped-nil-decl-reject — N1's UNTYPED leg: `var x = nil` / `x := nil` /
// `let x = nil` — a local declaration with NO ascription.
//
// PURPOSE: pin the checker reject for declarations whose only initializer is
// an untyped `nil`. The ascribed E4001 sites (`var s []int = nil`) all
// require a declared type to prove the nil against; with no ascription
// `shape_of_expr(nil)` is Unknown, the checker used to accept the
// declaration with 0 diagnostics, and the emitter emitted `var x = nil` —
// which Go rejects outright (`use of untyped nil in variable declaration`).
// Accepted-but-unbuildable (GUARANTEES row 13, BUILDABLE). nil has exactly
// two homes (N1): `error` and the foreign-Go perimeter, and both must be
// visible in a DECLARED type — an untyped nil can never name either.
//
// Distinct from 28-composite-nil-reject (nils NESTED in composite literals,
// which carry their own field/slot type context) and from the ascribed
// direct-nil sites (`var s []int = nil` — E4001, a declared type exists).
// Distinct from local `const c = nil` (the E6003 constant-expression guard,
// issue #640 — Go: "nil is not constant", a different Go error).
//
// Shapes pinned (one active, the rest commented alternates — same verdict):
//   var ch = nil      /  ch := nil  /  let x = nil  /  a, b := nil, 1
//   closure-body `var x = nil` (the closure boundary walk carries the check)
// Controls that must stay ACCEPTED (do not add to this file):
//   var e error = nil   — ascribed; error is nil's home (E4001 skips it)
//   f(nil) / ch == nil  — call argument / comparison, not a declaration
//   var ch chan int     — declare without initializer: the zero value (N3)
//
// EXPECTED-VERDICT: REJECT N1
// EXPECTED-DIAGNOSTIC: HYB-E4003
// ============================================================================

fn main() {
  // S1: the short define — Go: `use of untyped nil in variable declaration`.
  ch := nil
  _ = ch

  // S2 alternates (same verdict, same code):
  //   var ch2 = nil
  //   let x = nil
  //   a, b := nil, 1
  //   g := fn() { var y = nil; _ = y }; g()
}
