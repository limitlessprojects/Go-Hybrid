// ============================================================================
// any-operations-controls-accept — Checker corpus (bounty track: "grow the adversarial corpus").
//
// PURPOSE: the ACCEPT half of the #682 U9-U12 rules: every legal use of `any` that the
// new Interface classification must NOT reject.
//
// RULE CITATION: GUARANTEES.md Row 13 (BUILDABLE) — every `hyb check`-accepted
// file must build cleanly with `go build`.
//
// ---- Why this is a ACCEPT ----
// `any` is legal in many positions, and a rule that rejects those is a
// regression, not a fix. Each shape was verified to compile under `go build`
// before this fixture was committed, and the matching rejections are pinned by
// the four `any-*-reject.hy` fixtures beside it.
//
//   1. `var a any = 1` then `a = "str"` — an interface target accepts any
//      value, so assignment INTO `any` must stay allowed.
//   2. `fmt.println("{v}")` — a Field access whose base is the `fmt` PACKAGE,
//      not an interface. This is the false positive a careless field-access
//      check produces first, so it is pinned here deliberately.
//   3. `fn box() any { return g }` — a concrete value returned from an `any`
//      result.
//   4. `[]any{1, "two"}` and `map[string]any{}` — `any` as a container element
//      and value type.
//   5. `q.v = 5` where the field is declared `any`.
//   6. `take(1)` and `task worker(v, done)` — `any` as a call and task-spawn
//      argument. The corpus's own `any-erasure-primitive-control.hy` pins the
//      spawn case as ACCEPT (VERDICTS.md A1c), so it must stay one.
//   7. `a == 1` — Go DOES allow `==` between an interface and a non-interface
//      value (the non-interface side converts). Only the ORDERED comparisons
//      are illegal, which is why the `==`/`!=` arm is excluded from the rule.
//
// ---- Expected verdict (reproduced as a real `go build` failure first) ----
// EXPECTED-VERDICT: ACCEPT
//
// ---- Paired fixture ----
// `any-operations-controls-accept.hy` holds the legal uses of `any`, so these
// rejections cannot be widened into a rule that refuses buildable programs.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

fn box() any {
    var g int = 1
    return g
}

fn take(v any) {
    fmt.println("taken")
}

fn worker(v any, done chan<- bool) {
    done <- true
}

struct s {
    v any
}

fn main() {
    var a any = 1
    a = "str"
    fmt.println("{a}")

    var v any = 42
    fmt.println("{v}")

    x := box()
    fmt.println("{x}")

    xs := []any{1, "two"}
    fmt.println("{len(xs)}")

    m := map[string]any{}
    m["k"] = 1
    fmt.println("{len(m)}")

    var q s
    q.v = 5
    fmt.println("{q.v}")

    take(1)

    done := make(chan bool, 1)
    task worker(v, done)
    <-done

    if a == 1 {
        fmt.println("eq")
    }
}
