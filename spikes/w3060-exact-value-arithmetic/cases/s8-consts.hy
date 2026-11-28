// W-3060 shape s8: the SAME numbers as typed int8 CONSTANTS.
// Pre-registered: rejected by `hyb check` (Go folds the constant and refuses).
const a int8 = 100
const b int8 = 2
fn main() {
    _ = a * b
}
