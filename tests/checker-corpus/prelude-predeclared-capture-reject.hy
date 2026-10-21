// ============================================================================
// prelude-predeclared-capture-reject — Checker corpus (bounty track: "grow the adversarial corpus").
//
// PURPOSE: a user top-level declaration that shadows a Go PREDECLARED
// identifier the emitted prelude's own body calls.
//
// RULE CITATION: GUARANTEES.md Row 13 (BUILDABLE) — every `hyb check`-accepted
// file must build cleanly with `go build`.
//
// ---- Why this is a REJECT ----
// This is the second shape of the same issue, and it is NOT a redeclaration.
// A package-level declaration shadows a predeclared identifier for the whole
// Go file. SET_PRELUDE's own body is
//
//     func (s set[t]) len() int { return len(s.m) }
//
// so a user `fn len` captures that unqualified `len(s.m)` call, and Go refuses
// the result:
//
//     out.go:31:13: cannot use s.m (variable of type map[t]struct{}) as
//                   []int value in argument to len
//
// The same mechanism reaches `make` and `delete` through SET_PRELUDE
// (`make(map[t]struct{})`, `delete(s.m, v)`) and `panic` through
// OPTION_PRELUDE (`panic("HY-PANIC/unwrap-none")`).
//
// The checker previously accepted the file with 0 diagnostics.
//
// ---- Gate ----
// Rejected only when the set surface is actually used, because that is what
// injects the prelude. `fn len` in a program with no set surface emits no
// prelude and stays ACCEPT — see the paired controls fixture.
//
// ---- Expected verdict (reproduced as a real `go build` failure first) ----
// EXPECTED-VERDICT: REJECT BUILDABLE
// EXPECTED-DIAGNOSTIC: HYB-E1002
//
// ---- Paired fixtures ----
// `prelude-name-collision-reject.hy` (redeclaration) and
// `prelude-name-collision-controls-accept.hy` (the gated accept side).
// ============================================================================

fn len(xs []int) int {
    return 42
}

fn main() {
    s := set[int]{}
    s.insert(1)
    fmt.println("{len([]int{1})}")
}
