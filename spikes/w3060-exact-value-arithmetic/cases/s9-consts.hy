// W-3060 shape s9: the SAME numbers as typed int32 CONSTANTS.
// Pre-registered: rejected by `hyb check` (Go folds the constant and refuses).
const a int32 = 1
const b int32 = 31
fn main() {
    _ = a << b
}
