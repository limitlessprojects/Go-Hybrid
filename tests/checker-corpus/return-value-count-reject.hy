// ============================================================================
// return-value-count-reject — Checker corpus (bounty track: "grow the
// adversarial corpus").
//
// PURPOSE: a `return` that supplies the wrong number of values for its
// signature, with no sugar available to fill the gap. The emitted Go cannot
// build, so accepting it breaks GUARANTEES.md Row 13 (BUILDABLE).
//
// RULE CITATION: Row 13. Go refuses the statement:
//   `not enough return values` / `too many return values`
//
// ---- Why this is a REJECT ----
// `f` declares two unnamed results and `return 1` supplies one value. Neither
// of the two legal short-return shapes applies here:
//   * Go's naked `return` needs EVERY result to be named (corpus 63);
//   * the emitter's zero-fill needs the LAST result to be `error`, which is
//     E3's sugar (corpus 13 emits `return v, nil`).
// So Go requires the full arity and the checker previously accepted the file
// with 0 diagnostics. Closed by the #577 C10 count rule (HYB-E1002,
// `buildable.rs::check_return`).
//
// ---- Expected verdict (reproduced as a real `go build` failure first) ----
// EXPECTED-VERDICT: REJECT BUILDABLE
// EXPECTED-DIAGNOSTIC: HYB-E1002
//
// ---- Paired control ----
// `call-and-return-controls-accept.hy` holds `return two()` (Go's multi-value
// rule), the all-named naked `return`, and the `(value, error)` short return,
// so this rejection cannot be widened into a rule that refuses the sugar.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

fn f() (int, int) {
    return 1
}

fn main() {
    a, b := f()
    fmt.println("{a} {b}")
}
