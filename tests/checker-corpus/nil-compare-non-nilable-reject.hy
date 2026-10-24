// ============================================================================
// nil-compare-non-nilable-reject — Checker corpus (issue #1567).
//
// PURPOSE: `== nil` / `!= nil` against an operand whose type has NO nil value
// in Go. `check_equality` used to return early on any nil literal ("E4001/N1
// own its validity"), but E4001/N1 own nil at DECLARATION / assignment /
// return / spawn sites — a comparison is none of those — so these shapes
// passed `hyb check` and then failed `go build`.
//
// RULE CITATION: GUARANTEES.md Row 13 (BUILDABLE) — every `hyb check`-accepted
// file must build cleanly with `go build`.
//
// ---- Why this is a REJECT ----
// Go: `invalid operation: x == nil (mismatched types int and untyped nil)`.
// The shape is KNOWN (primitive / string / struct / array), so this is not an
// `Unknown` fail-open case: nil exists only for pointer, chan, func, map,
// slice and interface types.
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: REJECT N1
// EXPECTED-DIAGNOSTIC: HYB-E4001
//
// ---- Paired fixture ----
// `nil-compare-controls-accept.hy` carries the nilable half (slice, map, chan,
// func, pointer, `any`, `error`).
// ============================================================================

struct s {
    a int
}

fn main() {
    // primitive
    x := 1
    if x == nil {
        fmt.println("int")
    }

    // string
    str := "a"
    if str == nil {
        fmt.println("string")
    }

    // bool, through `!=`
    b := true
    if b != nil {
        fmt.println("bool")
    }

    // a struct value
    var v s
    if v == nil {
        fmt.println("struct")
    }

    // an array value
    var arr [2]int
    if arr == nil {
        fmt.println("array")
    }

    // the reversed operand order takes the same path
    if nil == x {
        fmt.println("reversed")
    }
}
