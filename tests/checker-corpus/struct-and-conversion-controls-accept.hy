// ============================================================================
// struct-and-conversion-controls-accept — Checker corpus (bounty track: "grow the adversarial corpus").
//
// PURPOSE: the ACCEPT half of the #682 U13-U16 rules: every shape here is legal Go that
// the new conversion-arity, struct-literal and field-assignment checks must NOT
// reject.
//
// RULE CITATION: GUARANTEES.md Row 13 (BUILDABLE) — every `hyb check`-accepted
// file must build cleanly with `go build`.
//
// ---- Why this is a ACCEPT ----
// These are accept-side checks: a rule that closes a bypass by rejecting
// buildable programs is a regression, not a fix. Each shape was verified to
// compile under `go build` before this fixture was committed, and the matching
// rejections are pinned by the three `*-reject.hy` fixtures beside it.
//
//   1. `int(x)` / `string(65)` — a conversion with exactly one argument.
//   2. `append(xs, 2, 3)` — a variadic builtin keeps its own arity rules.
//   3. `s{a: 1}` and `s{a: 1, b: 2}` — declared fields, including a subset,
//      which Go allows.
//   4. `z.f = 1` — an untyped int constant IS assignable to a `float64` field,
//      which is why `Int` into a float target must stay allowed. The kind model
//      cannot separate an untyped constant from an `int` variable, so rejecting
//      that direction would reject this program.
//   5. `z.q = r` with `r` of the SAME struct type — legal.
//   6. `a = "str"` where `a` is `any` — an interface target takes any value.
//   7. `x, y := two()` — a multi-value binding is not a single-target
//      assignment, so the new check must not fire on it.
//
// ---- Expected verdict (reproduced as a real `go build` failure first) ----
// EXPECTED-VERDICT: ACCEPT
//
// ---- Paired fixture ----
// `struct-and-conversion-controls-accept.hy` holds the legal neighbours, so
// these rules cannot be widened into something that refuses buildable programs.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

struct p {
    n int
}

struct s {
    a int
    b int
    f float64
    q p
}

fn two() (int, int) {
    return 1, 2
}

fn main() {
    v := int(1)
    w := string(65)
    fmt.println("{v} {w}")

    xs := []int{1}
    ys := append(xs, 2, 3)
    fmt.println("{len(ys)}")

    one := s{a: 1}
    both := s{a: 1, b: 2}
    fmt.println("{one.a} {both.b}")

    var z s
    z.f = 1
    fmt.println("{z.f}")

    var r p
    z.q = r
    fmt.println("{z.q.n}")

    var a any = 1
    a = "str"
    fmt.println("{a}")

    x, y := two()
    fmt.println("{x} {y}")
}
