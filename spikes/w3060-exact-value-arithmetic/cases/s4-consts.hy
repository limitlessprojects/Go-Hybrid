// W-3060 shape s4: the SAME numbers as typed int64 CONSTANTS.
// Pre-registered: rejected by `hyb check` (Go folds the constant and refuses).
const a int64 = 9223372036854775807
const b int64 = 1
fn main() {
    _ = a + b
}
