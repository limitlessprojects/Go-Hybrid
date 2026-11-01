// W-3060 shape s3: `2000000000 + 2000000000` on typed int32 VARIABLES.
// Pre-registered: accepted by `hyb check` (0 diagnostics); wraps at run time (X1/X2).
fn main() {
    var a int32 = 2000000000
    var b int32 = 2000000000
    c := a + b
    fmt.println(c)
}
