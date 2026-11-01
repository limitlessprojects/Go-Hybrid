// W-3060 shape s5: `200 + 100` on typed uint8 VARIABLES.
// Pre-registered: accepted by `hyb check` (0 diagnostics); wraps at run time (X1/X2).
fn main() {
    var a uint8 = 200
    var b uint8 = 100
    c := a + b
    fmt.println(c)
}
