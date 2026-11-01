// W-3060 shape s8: `100 * 2` on typed int8 VARIABLES.
// Pre-registered: accepted by `hyb check` (0 diagnostics); wraps at run time (X1/X2).
fn main() {
    var a int8 = 100
    var b int8 = 2
    c := a * b
    fmt.println(c)
}
