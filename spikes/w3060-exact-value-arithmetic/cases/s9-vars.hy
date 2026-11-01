// W-3060 shape s9: `1 << 31` on typed int32 VARIABLES.
// Pre-registered: accepted by `hyb check` (0 diagnostics); wraps at run time (X1/X2).
fn main() {
    var a int32 = 1
    var b int32 = 31
    c := a << b
    fmt.println(c)
}
