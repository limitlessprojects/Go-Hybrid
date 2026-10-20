// ============================================================================
// call-arity-mismatch-reject — Checker corpus (bounty track: "grow the
// adversarial corpus").
//
// PURPOSE: a call to a top-level `fn` that passes the wrong number of
// arguments. The emitted Go call cannot build, so accepting it breaks
// GUARANTEES.md Row 13 (BUILDABLE).
//
// RULE CITATION: Row 13 — every `hyb check`-accepted file builds cleanly with
// `go build`. Go refuses both directions at the call site:
//   `not enough arguments in call to add`
//   `too many arguments in call to add`
//
// ---- Why this is a REJECT ----
// `add` is declared `fn add(a int, b int) int`, so Go's front-end requires
// exactly two arguments. The checker previously accepted `add(1)` with 0
// diagnostics and the emitted Go failed to build. Closed by the #577
// C9 arity rule (HYB-E1002, `buildable.rs::check_user_fn_arity`).
//
// ---- Expected verdict (reproduced as a real `go build` failure first) ----
// EXPECTED-VERDICT: REJECT BUILDABLE
// EXPECTED-DIAGNOSTIC: HYB-E1002
//
// ---- Paired control ----
// `call-and-return-controls-accept.hy` holds the same shapes with the right
// argument counts, plus the method and closure arity controls, so this
// rejection cannot be widened into a blanket rule.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

fn add(a int, b int) int {
    return a + b
}

fn main() {
    r := add(1)
    fmt.println("{r}")
}
