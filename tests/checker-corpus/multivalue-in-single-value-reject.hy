// ============================================================================
// multivalue-in-single-value-reject — Checker corpus (bounty track: "grow the adversarial corpus").
//
// PURPOSE: a multi-value call used as an operand.
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
// Go: `multiple-value two() (value of type (int, int)) in single-value context`.
//
// ---- Expected verdict (reproduced as a real `go build` failure first) ----
// EXPECTED-VERDICT: REJECT BUILDABLE
// EXPECTED-DIAGNOSTIC: HYB-E2002
//
// Registration note (merged tree): the landed T23 multi-value binding rule
// (pr-599/#669) classifies this shape first — a 1-name binding whose
// initializer contains a multi-value call is E2002 — so the leading
// diagnostic is HYB-E2002, with this rule's HYB-E1002 co-emitted (pinned by
// `multivalue_and_void_positions.rs::multi_value_call_as_an_operand_is_rejected`).
// Both rules reject a program Go refuses; the verdict-level agreement is
// REJECT either way.
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

fn main() {
    v := two() + 1
    fmt.println("{v}")
}
