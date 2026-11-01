// ============================================================================
// issue_1682_const_bool_conv_accept — Corpus fixture for issue #1682.
//
// PURPOSE: verify identity constant conversions `bool(true)` / `bool(false)`
// and aliased `my_bool(true)` / `my_bool(false)` remain accepted with zero
// diagnostics.
//
// EXPECTED-VERDICT: ACCEPT
// ============================================================================

type my_bool = bool

fn main() {
    t := bool(true)
    f := bool(false)
    mt := my_bool(true)
    mf := my_bool(false)
    if t && !f && mt && !mf {
        return
    }
}
