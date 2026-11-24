// EXPECTED-VERDICT: ACCEPT
// Registered 2026-11-24 before running this fixture against the checker.
// Rationale: issue #4751 controls. The width comparison must not
// over-reject: the same width on both sides accepts, true aliases collapse
// to one canonical width (`byte` is `uint8`), and a declared type alias
// (`my_int = int64`) carries the width of its target. This guards against
// a fix that compares spellings instead of canonical widths, or that
// rejects every fn value whose parameter is a sized integer.
type my_int = int64
fn g(x int64) int {
    return 1
}
fn h(x uint8) int {
    return 1
}
fn main() {
    var f fn(int64) int = g
    var b fn(byte) int = h
    var a fn(my_int) int = g
    _ = f
    _ = b
    _ = a
}
