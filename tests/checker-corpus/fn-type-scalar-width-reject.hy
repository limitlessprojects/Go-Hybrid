// EXPECTED-VERDICT: REJECT BUILDABLE
// EXPECTED-DIAGNOSTIC: HYB-E1002
// Registered 2026-11-24 before running this fixture against the checker.
// Rationale: issue #4751. `g` takes an int64, but the fn-typed variable
// declares `fn(int) int`. The kinds match (Int vs Int), so the pre-fix
// checker accepted the assignment and `go build` failed with `cannot use g
// (variable of type func(int64) int) as func(int) int value`. Function-type
// compatibility must compare scalar widths, not just kinds: the same
// width-only gap reproduces at assignment, call argument, return, struct
// literal, map value, and fn-literal positions (pinned by unit tests in
// hy-check/tests/fn_type_scalar_widths.rs); this fixture pins the
// var-init position, the issue's minimal repro.
fn g(x int64) int {
    return 1
}
fn main() {
    var f fn(int) int = g
    _ = f
}
