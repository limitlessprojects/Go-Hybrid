// ============================================================================
// void-as-value-reject — Checker corpus (bounty track: "grow the adversarial corpus").
//
// PURPOSE: a call that produces no value, used as one.
//
// RULE CITATION: GUARANTEES.md Row 13 (BUILDABLE) — every `hyb check`-accepted
// file must build cleanly with `go build`.
//
// ---- Why this is a REJECT ----
// All three classes in this batch share one root cause: `kind_of_expr` on a call
// returned the FIRST result kind for a multi-value call (silently discarding
// the rest) and `OpKind::Void` for a no-result call (which no operand check
// rejected). The fix derives the call's value count in one place
// (`non_single_value_count`) and rejects it in the positions that need one.
//
// Go: `v() (no value) used as value`.
//
// This class was already latent in the repo: the T22 fixture in
// `hy-check/src/lib.rs` used `is_ok(delete(alias, "k"))` to get a `delete` into an
// `if` condition, and `delete` produces no value either — so that fixture was
// itself accepted-but-unbuildable. This rule surfaced it, and the fixture was
// rewritten to put the `delete` in a closure body (statement position) with the
// closure called in the condition. Both of its assertions stayed strict.
//
// ---- Expected verdict (reproduced as a real `go build` failure first) ----
// EXPECTED-VERDICT: REJECT BUILDABLE
// EXPECTED-DIAGNOSTIC: HYB-E1002
//
// ---- Paired fixture ----
// `multivalue-and-void-controls-accept.hy` holds the legal multi-value and
// no-result forms, so these rejections cannot be widened into a rule that
// refuses buildable programs.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

fn v() {
    fmt.println("x")
}

fn main() {
    a := v()
    fmt.println("{a}")
}
