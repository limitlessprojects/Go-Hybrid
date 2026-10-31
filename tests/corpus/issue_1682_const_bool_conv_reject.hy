// ============================================================================
// issue_1682_const_bool_conv_reject — Corpus fixture for issue #1682.
//
// PURPOSE: verify `hyb check` rejects non-bool constant conversions to `bool`
// and aliased `bool` conversion targets with `HYB-E6006`, matching Go's
// compile-time rejection (`cannot convert ... to type bool`).
//
// EXPECTED-VERDICT: REJECT BUILDABLE
// ============================================================================

type my_bool = bool

fn main() {
    b1 := bool(1)
    b2 := bool(0)
    b3 := bool(3.14)
    b4 := bool("true")
    b5 := bool('a')
    b6 := bool(nil)
    b7 := my_bool(1)
    b8 := (bool)("false")
    _ = b1
    _ = b2
    _ = b3
    _ = b4
    _ = b5
    _ = b6
    _ = b7
    _ = b8
}
