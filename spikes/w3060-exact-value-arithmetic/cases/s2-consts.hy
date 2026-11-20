// W-3060 shape s2: the SAME numbers as typed int16 CONSTANTS.
// Pre-registered: rejected by `hyb check` (Go folds the constant and refuses).
const a int16 = 32767
const b int16 = 1
fn main() {
    _ = a + b
}
