// ============================================================================
// 48-const-init-not-constant — Checker corpus #48 (corpus GROWTH, added AFTER
// the checker existed — the "adversarial corpus tests" track of BOUNTY.md).
//
// (Numbering note: #195's fix is PR #271 → row 46; #194's fix is PR #276 →
// row 47. This row is 48 so all three can land without a collision; renumber
// freely if they are taken in another order.)
//
// PURPOSE: pin the "the RHS is not a constant expression at all" half of the
// `const` contract (issue #205). Cases 26/28 and the #148 fix pinned constants
// that OVERFLOW a declared type (HYB-E6002) and the #37 fix pinned writes
// THROUGH a `const` root (HYB-E3006); none of them asks whether the initializer
// can be a Go constant in the first place.
//
// THE PROGRAM (from issue #205, verbatim):
//
//   fn get_val() int { return 42 }
//   fn main() {
//       const c = get_val()   // accepted with 0 diagnostics
//       fmt.println("c: {c}")
//   }
//
// `hy-emit` keeps the Go `const` keyword for every initializer that is not a
// composite literal (`SliceLit` / `MapLit` / `StructLit` render as `var` —
// issue #188), so this emitted literally `const c = get_val()` and cmd/compile
// refused it: `get_val() (value of type int) is not constant`. The checker
// performed no constant evaluation at all.
//
// The fix adds HYB-E6003 at both `const` declaration sites (file-level
// `Decl::Const` and the statement-level `LocalKind::Const`) and mirrors the
// emitter's composite test so a composite initializer stays out of the rule.
// Accepted initializers: literals (`int`/`float`/`string`/`bool`/`char`),
// unary and binary operations over constants, and names of `const` bindings in
// scope. Not accepted: calls, index/field reads, and anything else the checker
// cannot evaluate.
//
// THE BUG (before the fix, on c09e9928): `hyb check` printed
// `OK (0 diagnostics)`; `hyb build --emit-go` then produced Go that `go build`
// rejected (issue #205 evidence, darwin-arm64). This fixture pins the
// check-side verdict.
//
// UNCONTESTED: SPEC §1.1/§2.4 — `const` is the compile-time constant binding
// (the only deep immutability in MVP), and the accepted-should-emit-buildable
// contract makes an accepted-but-unbuildable program a checker bug.
//
// ---- Tier-0 rule set applied to each value ----
// Layer 1 (Go-side GC): none — `get_val` returns a primitive and nothing
//   crosses a task or channel boundary.
// Layer 2 (Rust-side owned): none.
// Layer 3 (boundary handles): none.
//
// ---- Sendability / check sites (task spawn / channel send) ----
//   None. This is a declaration-form rule, not a boundary rule.
//
// ---- Expected verdict (registered 2026-10-16, before running the fixture
// against the fixed checker) ----
// EXPECTED-VERDICT: REJECT BUILDABLE
//
// A `const` bound to a call is not a compile-time constant; the checker must
// reject it instead of accepting a program the emitter cannot build.
// ============================================================================

fn get_val() int {
    return 42
}

fn main() {
    const c = get_val()
    fmt.println("c: {c}")
}
