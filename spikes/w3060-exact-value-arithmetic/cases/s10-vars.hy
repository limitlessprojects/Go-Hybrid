// W-3060 shape s10: `-128 - 1` on typed int8 VARIABLES.
// Pre-registered: accepted by `hyb check` (0 diagnostics); wraps at run time (X1/X2).
fn main() {
    var a int8 = -128
    var b int8 = 1
    c := a - b
    fmt.println(c)
}
