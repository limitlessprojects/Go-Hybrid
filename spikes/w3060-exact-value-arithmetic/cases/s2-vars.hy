// W-3060 shape s2: `32767 + 1` on typed int16 VARIABLES.
// Pre-registered: accepted by `hyb check` (0 diagnostics); wraps at run time (X1/X2).
fn main() {
    var a int16 = 32767
    var b int16 = 1
    c := a + b
    fmt.println(c)
}
