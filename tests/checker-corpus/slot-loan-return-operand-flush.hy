// EXPECTED-VERDICT: ACCEPT
// EXPECTED-OUTPUT: bump: 11 11 try: 42
// Slot loans (O8/OQ) must be written back BETWEEN operand evaluation and
// the `return` itself, and before every early return (issue #1398).
// `return c.next()` mutates *through* the loan while its operands are
// being evaluated; the old top-of-return flush emitted `m["k"] = c`
// BEFORE the call, so the post-call map kept the stale pre-call value
// and `bump` printed `11 10` instead of `11 11`. `update_then_try` pins
// the same store-back on the `?` error-propagation exit, where the
// mutation performed before the failing call used to vanish entirely
// (printed `10`, must print `42`). Tier-0: map/struct/ctrl plus error
// propagation only - no spawn, hatch, channel, foreign boundary, or
// managed directive; `hyb check --deny-managed` is clean.
// Pre-registered 2026-10-24 reproducing issue #1398 on main @ 7d63d5f,
// whose generated Go was inspected to confirm the flush landed before
// the operand call and was missing from the propagate-error path.

import "errors"
import "fmt"

struct counter {
    n int
}

fn (c *counter) next() int {
    c.n = c.n + 1
    return c.n
}

fn bump(m map[string]counter) int {
    c := &m["k"]
    return c.next()
}

fn fail_step() (int, error) {
    return 0, errors.new("boom")
}

fn update_then_try(m map[string]counter) (int, error) {
    c := &m["k"]
    c.n = 42
    x := fail_step()?
    c.n = c.n + x
    return c.n, nil
}

fn main() {
    m1 := map[string]counter{"k": counter{n: 10}}
    got := bump(m1)
    m2 := map[string]counter{"k": counter{n: 10}}
    _, _ = update_then_try(m2)
    fmt.println("bump:", got, m1["k"].n, "try:", m2["k"].n)
}
