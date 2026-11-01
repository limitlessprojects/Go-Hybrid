// W-3060 shape s7: the SAME numbers as typed uint64 CONSTANTS.
// Pre-registered: rejected by `hyb check` (Go folds the constant and refuses).
const a uint64 = 18446744073709551615
const b uint64 = 1
fn main() {
    _ = a + b
}
