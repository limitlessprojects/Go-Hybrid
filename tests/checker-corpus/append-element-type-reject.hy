// ============================================================================
// append-element-type-reject — Checker corpus (bounty track: "grow the adversarial corpus").
//
// PURPOSE: `append` with a value that is not assignable to the slice's element type.
//
// RULE CITATION: GUARANTEES.md Row 13 (BUILDABLE) — every `hyb check`-accepted
// file must build cleanly with `go build`.
//
// ---- Why this is a REJECT ----
// Go: `cannot use "str" (untyped string constant) as int value in argument to
// append`.
//
// The rule compares each appended value against the slice's element kind. It
// skips a variadic spread (`append(a, b...)` passes the whole slice, not an
// element) and stays fail-open on an unknown element kind.
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

fn main() {
    xs := []int{1}
    ys := append(xs, "str")
    fmt.println("{ys}")
}
