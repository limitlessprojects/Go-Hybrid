// ============================================================================
// call-and-return-controls-accept — Checker corpus (bounty track: "grow the
// adversarial corpus").
//
// PURPOSE: the ACCEPT half of the #577 C9/C10 call-and-return rule. Every
// shape here is legal Go that the new arity and return checks must NOT
// reject. The rule is accept-side: it exists to stop the checker accepting
// programs Go refuses, and a version of it that refuses buildable programs is
// a regression, not a fix.
//
// RULE CITATION: GUARANTEES.md Row 13 (BUILDABLE) — accepted programs must
// build. Each shape below was verified to compile under `go build` before
// this fixture was committed, and the corresponding rejection is pinned by
// `call-arity-mismatch-reject.hy`, `return-value-count-reject.hy`, and
// `return-type-mismatch-reject.hy`.
//
// ---- Why each shape is an ACCEPT ----
// 1. `add(1, 2)` — one argument per declared parameter.
// 2. `x.bump(2)` — the receiver is not an argument, so a one-parameter
//    method takes one argument.
// 3. `f(1)` on a closure — a closure states its own parameter count, and the
//    right count is accepted.
// 4. `naked()` — Go's naked `return` is legal exactly when EVERY result is
//    named, which is the corpus-63 shape.
// 5. `short()` — the emitter zero-fills a short return whose last result is
//    `error` (`return 1` emits `return 1, nil`), the corpus-13 E3 sugar.
// 6. `two()` via `return two()` — Go's multi-value rule: a single call whose
//    results line up exactly with the enclosing signature.
// 7. `floaty()` returning `1` — an untyped int constant IS assignable to a
//    `float64` result, so this direction must stay allowed. The kind model
//    cannot separate an untyped constant from an `int` variable, so rejecting
//    `int` for a float result would reject this program too.
// 8. `sliced()` returning a matching `[]int`.
// 9. `anyval()` returning an `int` from an `any` result.
//
// ---- Expected verdict (verified on this branch before committing) ----
// EXPECTED-VERDICT: ACCEPT
//   ACCEPT — 0 annotations, 0 managed. `hyb check` accepts it AND the emitted
//   Go compiles; both were run before this fixture was committed.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

struct counter {
    n int
}

fn (c *counter) bump(k int) int {
    c.n = c.n + k
    return c.n
}

fn add(a int, b int) int {
    return a + b
}

fn naked() (n int) {
    n = 7
    return
}

fn short() (int, error) {
    return 1
}

fn two() (int, int) {
    return 1, 2
}

fn forward() (int, int) {
    return two()
}

fn floaty() float64 {
    return 1
}

fn sliced() []int {
    xs := []int{1, 2}
    return xs
}

fn anyval() any {
    var g int = 1
    return g
}

fn main() {
    r := add(1, 2)
    var c counter
    m := c.bump(2)
    f := fn(x int) int {
        return x + 1
    }
    q := f(1)
    fmt.println("{r} {m} {q}")
    fmt.println("{naked()}")
    v, e := short()
    fmt.println("{v} {e}")
    a, b := forward()
    fmt.println("{a} {b}")
    fmt.println("{floaty()}")
    fmt.println("{len(sliced())}")
    fmt.println("{anyval()}")
}
