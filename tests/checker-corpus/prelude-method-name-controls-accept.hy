// ============================================================================
// prelude-method-name-controls-accept — Checker corpus (bounty track: "grow the adversarial corpus").
//
// PURPOSE: the ACCEPT half of #777 for a name that DOES inject the prelude but
// still cannot collide, because a method and a struct-associated function do
// not share Go's package-scope namespace with it.
//
// RULE CITATION: GUARANTEES.md Row 13 (BUILDABLE) — every `hyb check`-accepted
// file must build cleanly with `go build`.
//
// ---- Why this is an ACCEPT ----
// Both declarations below put the literal text `some(` into the emitted Go, so
// `needs_option` fires and OPTION_PRELUDE is injected — including its
// package-scope `func some[v any](val v) option[v]`. Nothing collides anyway:
//
//   1. `fn (p pair) some() int` lowers to `func (p pair) some() int`. A method
//      lives in the method set of its receiver type, not the package scope, so
//      it does not redeclare the prelude's function. (A `fn some` at package
//      scope does — see `prelude-name-collision-reject.hy`.)
//   2. `fn holder.some(x int) int` lowers to the package-scope name
//      `holder_some`, which is a different identifier from `some`.
//
// The rule therefore has to distinguish a package-scope function declaration
// from a method or a struct-associated `fn S.f`; a rule keyed on the bare name
// alone rejects these two buildable programs.
//
// ---- Expected verdict (verified to build and run before commit) ----
// EXPECTED-VERDICT: ACCEPT
//
// ---- Paired fixtures ----
// `prelude-name-collision-reject.hy` (the package-scope twin) and
// `prelude-predeclared-capture-reject.hy` (predeclared capture).
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

struct pair {
    a int
    b int
}

fn (p pair) some() int {
    return p.a
}

struct holder {
    n int
}

fn holder.some(x int) int {
    return x + 1
}

fn main() {
    q := pair{a: 1, b: 2}
    fmt.println("{q.some()} {holder.some(9)}")
}
