// W-3060 shape s6: the SAME numbers as typed uint32 CONSTANTS.
// Pre-registered: rejected by `hyb check` (Go folds the constant and refuses).
const a uint32 = 4000000000
const b uint32 = 1000000000
fn main() {
    _ = a + b
}
