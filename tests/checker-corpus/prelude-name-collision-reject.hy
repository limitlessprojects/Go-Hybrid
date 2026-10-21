// ============================================================================
// prelude-name-collision-reject — Checker corpus (bounty track: "grow the adversarial corpus").
//
// PURPOSE: a user top-level declaration reusing a name the emitted prelude
// declares at package scope.
//
// RULE CITATION: GUARANTEES.md Row 13 (BUILDABLE) — every `hyb check`-accepted
// file must build cleanly with `go build`.
//
// ---- Why this is a REJECT ----
// The emitter injects OPTION_PRELUDE into the same Go file as the user's
// declarations whenever the emitted body mentions the option surface. That
// prelude declares `type option[v any] struct` and
// `func some[v any](val v) option[v]` at package scope, so a user `fn some`
// is a redeclaration:
//
//     out.go:39:6: some redeclared in this block
//     out.go:13:6: other declaration of some
//
// The checker previously accepted the file with 0 diagnostics.
//
// Note that this collides even though the checker's own name resolution keeps
// the two `some`s apart: the emitted Go has one package-scope namespace, and
// the prelude's `func some` is unconditional once it is injected.
//
// ---- Expected verdict (reproduced as a real `go build` failure first) ----
// EXPECTED-VERDICT: REJECT BUILDABLE
// EXPECTED-DIAGNOSTIC: HYB-E1002
//
// ---- Paired fixtures ----
// `prelude-predeclared-capture-reject.hy` is the sibling mechanism (a
// package-level builtin capture rather than a redeclaration), and
// `prelude-name-collision-controls-accept.hy` holds the legal neighbours —
// programs that declare the same names but never pull the prelude in, so they
// still build.
// ============================================================================

fn some(x int) int {
    return x + 1
}

fn has(o option[int]) bool {
    return o.is_some()
}

fn main() {
    n := some(1)
    fmt.println("{n}")
}
