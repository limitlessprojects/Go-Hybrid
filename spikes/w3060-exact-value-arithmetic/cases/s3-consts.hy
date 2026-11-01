// W-3060 shape s3: the SAME numbers as typed int32 CONSTANTS.
// Pre-registered: rejected by `hyb check` (Go folds the constant and refuses).
const a int32 = 2000000000
const b int32 = 2000000000
fn main() {
    _ = a + b
}
