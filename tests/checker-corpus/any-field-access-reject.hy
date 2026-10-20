// ============================================================================
// any-field-access-reject — Checker corpus (bounty track: "grow the adversarial corpus").
//
// PURPOSE: selecting a field from a value of type `any`.
//
// RULE CITATION: GUARANTEES.md Row 13 (BUILDABLE) — every `hyb check`-accepted
// file must build cleanly with `go build`.
//
// ---- Why this is a REJECT ----
// `any` used to resolve to `OpKind::Unknown`, and `Unknown` is fail-open
// everywhere by design (it means "the checker could not resolve this").
// `any` is the opposite: a RESOLVED type that Go refuses to operate on.
// The #682 fix adds `OpKind::Interface` for it, which puts it in the operand
// checks' reject set because those checks list the kinds they ACCEPT.
//
// Go: `a.nope undefined (type any has no field or method nope)`.
//
// This one needed an explicit check: an interface is not a struct, so the
// existing struct-field lookup could not catch it.
//
// ---- Expected verdict (reproduced as a real `go build` failure first) ----
// EXPECTED-VERDICT: REJECT BUILDABLE
// EXPECTED-DIAGNOSTIC: HYB-E1002
//
// ---- Paired fixture ----
// `any-operations-controls-accept.hy` holds the legal uses of `any`, so these
// rejections cannot be widened into a rule that refuses buildable programs.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

fn main() {
    var a any = 1
    b := a.nope
    fmt.println("{b}")
}
