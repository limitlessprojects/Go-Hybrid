// W-3060 shape s6: `4000000000 + 1000000000` on typed uint32 VARIABLES.
// Pre-registered: accepted by `hyb check` (0 diagnostics); wraps at run time (X1/X2).
fn main() {
    var a uint32 = 4000000000
    var b uint32 = 1000000000
    c := a + b
    fmt.println(c)
}
