// ============================================================================
// defer-arity-reject — Checker corpus (bounty track: "grow the adversarial corpus").
//
// PURPOSE: a deferred call whose argument count does not match the callee's parameters.
//
// RULE CITATION: GUARANTEES.md Row 13 (BUILDABLE) — every `hyb check`-accepted
// file must build cleanly with `go build`.
//
// ---- Why this is a REJECT ----
// Go: `not enough arguments in call to g`.
//
// `builtins.rs::check_stmt_defer_call` owns the "defer requires a CALL" half
// (HYB-E2001); this rule owns the arity half. It fires only for a bare
// identifier that resolves to a declared `fn`, so a builtin (`defer close(c)`)
// or a method (`defer q.stop()`) is left alone.
//
// ---- Expected verdict (reproduced as a real `go build` failure first) ----
// EXPECTED-VERDICT: REJECT BUILDABLE
// EXPECTED-DIAGNOSTIC: HYB-E1002
//
// ---- Paired fixture ----
// `member-defer-and-append-controls-accept.hy` holds the legal neighbours, so
// these rejections cannot be widened into a rule that refuses buildable
// programs.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

fn g(a int) {
    fmt.println("{a}")
}

fn main() {
    defer g()
}
