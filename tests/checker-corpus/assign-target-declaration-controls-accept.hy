// ============================================================================
// assign-target-declaration-controls-accept — Checker corpus (issue #1574).
//
// PURPOSE: every legal write target must keep working after the #1574 rule.
// The reject half is `assign-target-declaration-reject.hy`.
//
// RULE CITATION: GUARANTEES.md Row 13 (BUILDABLE) — accept side.
//
// ---- Why this is an ACCEPT ----
// A binding in any enclosing scope is a variable: `var`, `:=`, `let` after it
// is released from immutability? No — `let` stays immutable (E3006). A local
// binding also SHADOWS a package-scope name, so `len := 5; len = 6; len++` is
// a legal Go program. `_` is the blank target. A parameter is a variable.
// A function name in VALUE position is a function value (`v := f`) — only a
// store into it is illegal.
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: ACCEPT
// ============================================================================

fn f() int {
    return 1
}

fn g(x int) int {
    x = 2
    x++
    return x
}

fn main() {
    var y int
    y = 3
    y += 1

    _ = 1

    // a local binding shadows the predeclared `len`
    len := 5
    len = 6
    len++

    // a function name in value position is a function value
    v := f

    fmt.println(g(y) + len + v())
}
