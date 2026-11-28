// ============================================================================
// prelude-local-shadow-reject — Checker corpus (bounty track: "grow the
// adversarial corpus").
//
// PURPOSE: the REJECT half of issue #4695 variant E: a LOCAL binding named
// `hy_pop` captures the lowered `.pop()` call. `.pop()` on a slice lowers
// to a call of the prelude helper `hy_pop(&stack)` INSIDE the user's own
// function body, so a local `hy_pop := 1` in scope shadows the package-level
// helper and the emitted Go fails ("cannot call hy_pop (variable of type
// int): int is not a function"). Top-level declarations are a different
// shape (issue #777); this fixture pins ONLY the local-at-call-site
// mechanism, which no #777 fix covers.
//
// RULE CITATION: GUARANTEES.md Row 13 (BUILDABLE) — every `hyb check`-accepted
// file must build cleanly with `go build`.
//
// ---- Why this is a REJECT ----
// `hyb check` accepted it (0 diagnostics) and `hyb build` emitted Go that
// `go build` refused, with no Hybrid span pointing at the user declaration.
// The checker must reject the `.pop()` call site with HYB-E1002.
//
// ---- Expected verdict (reproduced as a real `go build` failure first) ----
// EXPECTED-VERDICT: REJECT BUILDABLE
// EXPECTED-DIAGNOSTIC: HYB-E1002
//
// ---- Paired fixture ----
// `prelude-local-shadow-controls-accept.hy` holds the legal neighbours, so
// this rejection cannot be widened into a rule that refuses buildable
// programs (local declared after the call, local confined to a sibling
// block, local with no `.pop()` use at all).
//
// ---- Annotations / managed ----
// None.
// ============================================================================

fn main() {
    hy_pop := 1
    stack := []int{1, 2, 3}
    popped := stack.pop()
    fmt.println("{hy_pop} {popped}")
}
