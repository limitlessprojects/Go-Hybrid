// ============================================================================
// member-defer-and-append-controls-accept — Checker corpus (bounty track: "grow the adversarial corpus").
//
// PURPOSE: the ACCEPT half of the #682 U2-U5 rules: every legal neighbour the new
// member, defer, and append checks must NOT reject.
//
// RULE CITATION: GUARANTEES.md Row 13 (BUILDABLE) — every `hyb check`-accepted
// file must build cleanly with `go build`.
//
// ---- Why this is a ACCEPT ----
// Each of the three checks has a legal neighbour that a careless version
// rejects, and two of them were caught that way during development. Each shape
// was verified to build before this fixture was committed.
//
//   1. A declared method call, a field read, and a function-typed field call —
//      the ordinary cases the member rule must still allow.
//   2. `stack.pop()` on a `[]int` — example 08's stack shape. `hy-emit` LOWERS
//      `.pop()` to the `hy_pop(&stack)` prelude helper rather than a Go method
//      call, and Go has no slice method, so a naive "no such member" rule
//      rejects it. The spike-6 positive suite caught that; the intrinsic list
//      lives in `buildable.rs::LOWERED_METHOD_INTRINSICS` and must be kept in
//      sync with the Field-name special cases in `hy-emit/src/expr.rs`.
//   3. `hits.add(1)` / `hits.load()` on a `*atomic.int64` — a foreign generic
//      whose members the kind model cannot see, so it must stay fail-open.
//   4. `defer g(1)` with a matching argument count, `defer close(c)` for a
//      builtin, and `defer q.stop()` for a method.
//   5. `append(xs, 2, 3)` for multiple values, `append(s, "b")` on a string
//      slice, `append(a, b...)` as a variadic spread, and an untyped int
//      constant appended to a `[]float64` (which is legal, and is why `Int`
//      into a float element must stay allowed).
//
// ---- Expected verdict (reproduced as a real `go build` failure first) ----
// EXPECTED-VERDICT: ACCEPT
//
// ---- Paired fixture ----
// `member-defer-and-append-controls-accept.hy` holds the legal neighbours, so
// these rejections cannot be widened into a rule that refuses buildable
// programs.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

import "sync/atomic"

struct c {
    n int
}

fn (x *c) bump(k int) int {
    x.n = x.n + k
    return x.n
}

struct holder {
    a int
    f fn(a int) int
}

struct r {
    n int
}

fn (x *r) stop() {
    fmt.println("stopped")
}

fn g(a int) {
    fmt.println("{a}")
}

fn inc(a int) int {
    return a + 1
}

fn probe(hits *atomic.int64) {
    hits.add(1)
    fmt.println("{hits.load()}")
}

fn main() {
    var z c
    fmt.println("{z.bump(2)}")

    var h holder
    h.f = inc
    fmt.println("{h.a}")
    fmt.println("{h.f(1)}")

    stack := []int{1, 2}
    popped := stack.pop()
    fmt.println("{popped}")

    defer g(1)

    var q r
    defer q.stop()

    ch := make(chan int, 1)
    defer close(ch)

    xs := []int{1}
    fmt.println("{len(append(xs, 2, 3))}")

    ss := []string{"a"}
    bs := "b"
    fmt.println("{len(append(ss, bs))}")

    a := []int{1}
    b := []int{2}
    fmt.println("{len(append(a, b...))}")

    fs := []float64{}
    fmt.println("{len(append(fs, 1))}")
}
