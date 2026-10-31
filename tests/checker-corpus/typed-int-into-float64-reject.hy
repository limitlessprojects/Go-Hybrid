// ============================================================================
// typed-int-into-float64-reject — Checker corpus (bounty track: "grow the
// adversarial corpus").
//
// PURPOSE: a typed `int` VALUE flowing into a `float64` target — the value
// positions Go refuses with `cannot use ... as float64 value in ...`.
//
// RULE CITATION: GUARANTEES.md Row 13 (BUILDABLE) — every `hyb check`-accepted
// file must build cleanly with `go build`.
//
// ---- Why this is a REJECT ----
// `y` is declared `var y int = 3`, so it is a TYPED int, not an untyped
// constant. Go refuses every shape here at build time:
//   * `var x float64 = y` -> `cannot use y (value of type int) as float64
//     value in variable declaration`
//   * `x = y`              -> `cannot use y ... value in assignment`
//   * `x += y`             -> `invalid operation: x += y (mismatched types
//     float64 and int)`
// The checker previously accepted all of them (0 diagnostics), because
// `kinds_conflict`'s `(Float, Int) => false` arm — which exists solely for
// untyped int constants (`var f float64 = 1` builds) — cannot distinguish an
// untyped constant from an `int` variable. Issue #1112: the kind model needs
// the value expression to tell them apart (`value_conflicts` +
// `is_untyped_constant`).
//
// ---- Expected verdict (reproduced as a real `go build` failure first) ----
// EXPECTED-VERDICT: REJECT BUILDABLE
// EXPECTED-DIAGNOSTIC: HYB-E1002
//
// ---- Paired fixture ----
// `call-and-return-controls-accept.hy` pins the legal neighbour (`floaty()`
// returns the untyped constant `1`); the untyped-constant accept controls at
// every value position live in `hy-check/tests/typed_int_float_coercion_issue_1112.rs`,
// so this rule cannot be widened into something that refuses buildable
// programs.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

fn main() {
    var y int = 3
    var x float64 = y
    x = y
    x += y
    fmt.println("{x}")
}
