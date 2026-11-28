// W-3060 shape s7: `18446744073709551615 + 1` on typed uint64 VARIABLES.
// Pre-registered: accepted by `hyb check` (0 diagnostics); wraps at run time (X1/X2).
fn main() {
    var a uint64 = 18446744073709551615
    var b uint64 = 1
    c := a + b
    fmt.println(c)
}
