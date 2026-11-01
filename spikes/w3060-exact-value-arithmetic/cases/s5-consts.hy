// W-3060 shape s5: the SAME numbers as typed uint8 CONSTANTS.
// Pre-registered: rejected by `hyb check` (Go folds the constant and refuses).
const a uint8 = 200
const b uint8 = 100
fn main() {
    _ = a + b
}
