// ============================================================================
// nil-compare-controls-accept — Checker corpus (issue #1567).
//
// PURPOSE: every shape that IS nilable must keep comparing to nil. These are
// legal Go today and `hyb build` succeeds on each; the #1567 fix rejects only
// primitive / string / struct / array, so this file pins the boundary.
//
// RULE CITATION: GUARANTEES.md Row 13 (BUILDABLE) — the accept half: a rule
// that closes a bypass by rejecting buildable programs is a regression.
//
// ---- Why this is an ACCEPT ----
// Go: pointer, chan, func, map, slice and interface types are all comparable
// to nil. `var a any = 1` is the load-bearing case: the declared type is an
// interface even though the initializer's shape is primitive, so the check
// must resolve the DECLARED type and fail open (N1: nil's homes are `error`
// and the foreign perimeter).
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: ACCEPT
// ============================================================================

fn main() {
    var xs []int
    if xs == nil {
        fmt.println("slice")
    }

    var m map[string]int
    if m == nil {
        fmt.println("map")
    }

    var ch chan int
    if ch == nil {
        fmt.println("chan")
    }

    f := fn() {}
    if f == nil {
        fmt.println("func")
    }

    var p *int
    if p == nil {
        fmt.println("ptr")
    }

    var a any = 1
    if a == nil {
        fmt.println("any")
    }

    var e error = nil
    if e == nil {
        fmt.println("error")
    }
}
