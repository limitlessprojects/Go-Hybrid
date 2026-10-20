// ============================================================================
// multivalue-and-void-controls-accept — Checker corpus (bounty track: "grow the adversarial corpus").
//
// PURPOSE: the ACCEPT half of the #682 U6-U8 rules: every legal multi-value and no-result
// form that the new value-position check must NOT reject.
//
// RULE CITATION: GUARANTEES.md Row 13 (BUILDABLE) — every `hyb check`-accepted
// file must build cleanly with `go build`.
//
// ---- Why this is a ACCEPT ----
// The legal multi-value forms are easy to break, so each is pinned here and in
// `hy-check/tests/multivalue_and_void_positions.rs`. All were verified to build
// before the change, and the matching rejections are pinned by the three
// `*-reject.hy` fixtures beside it.
//
//   1. `a, b := two()` — the ordinary multi-value binding.
//   2. `a, b = two()` — the same in assignment form.
//   3. `return two()` from a `(int, int)` function — Go's multi-value rule.
//   4. `g(two())` where `g` really takes two parameters. This is the case that
//      makes the rule an arity comparison rather than a blanket rejection.
//   5. `v()` as a STATEMENT — the whole point of a no-result function. Also
//      pinned: `defer v()`, where the call is the defer operand, not a value.
//   6. `v, ok := m["k"]` and `v, ok := <-ch` — the other two two-value sources.
//      They are not calls, so the new rule must not touch them.
//   7. Single-value calls in every position: binding, operand, argument.
//
// ---- Expected verdict (reproduced as a real `go build` failure first) ----
// EXPECTED-VERDICT: ACCEPT
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

fn fwd() (int, int) {
    return two()
}

fn g(a int, b int) {
    fmt.println("{a} {b}")
}

fn v() {
    fmt.println("v ran")
}

fn one() int {
    return 1
}

fn main() {
    a, b := two()
    fmt.println("{a} {b}")

    var x int
    var y int
    x, y = two()
    fmt.println("{x} {y}")

    p, q := fwd()
    fmt.println("{p} {q}")

    g(two())

    v()
    defer v()

    m := map[string]int{}
    mv, ok := m["k"]
    fmt.println("{mv} {ok}")

    c := make(chan int, 1)
    c <- 7
    cv, cok := <-c
    fmt.println("{cv} {cok}")

    s := one()
    t := one() + 1
    g(one(), 2)
    fmt.println("{s} {t}")
}
