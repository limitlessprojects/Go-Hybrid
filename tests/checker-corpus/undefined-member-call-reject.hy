// ============================================================================
// undefined-member-call-reject — Checker corpus (bounty track: "grow the adversarial corpus").
//
// PURPOSE: a call to a member that does not exist on the base's type.
//
// RULE CITATION: GUARANTEES.md Row 13 (BUILDABLE) — every `hyb check`-accepted
// file must build cleanly with `go build`.
//
// ---- Why this is a REJECT ----
// Go: `x.nope undefined (type int has no field or method nope)`.
//
// The checker previously accepted the file with 0 diagnostics: the
// `check_call_operands` field branch only reported a member that EXISTS but is
// not callable, and stayed silent when nothing was found at all.
//
// The rule fails open on anything unresolved — see
// `is_fully_resolved` — because a foreign generic such as `*atomic.int64`
// resolves to `Pointer(Unknown)`, where the members are INVISIBLE to the kind
// model rather than absent. The corpus's own
// `17-atomic-channel-immutable.hy` pins that case as ACCEPT, and an early draft
// of this change rejected it.
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

struct s {
    a int
}

fn main() {
    var q s
    v := q.missing()
    fmt.println("{v}")
}
