// W-3060 shape s4: `9223372036854775807 + 1` on typed int64 VARIABLES.
// Pre-registered: accepted by `hyb check` (0 diagnostics); wraps at run time (X1/X2).
fn main() {
    var a int64 = 9223372036854775807
    var b int64 = 1
    c := a + b
    fmt.println(c)
}
