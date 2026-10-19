// ============================================================================
// multivalue-arg-arity-reject — Checker corpus (bounty track: "grow the adversarial corpus").
//
// PURPOSE: a lone multi-value call as the whole argument list of a callee that cannot take
// that many.
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
// Go: `too many arguments in call to g`.
//
// Note the asymmetry this fixture exists to pin: the SAME call is legal against a
// 2-parameter callee (`g(two())` builds, and the control fixture pins that). The
// rule compares the expansion count against the resolved arity, so it must not be
// written as a blanket rejection of a multi-value argument.
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

fn two() (int, int) {
    return 1, 2
}

fn g(a int) {
    fmt.println("{a}")
}

fn main() {
    g(two())
}
