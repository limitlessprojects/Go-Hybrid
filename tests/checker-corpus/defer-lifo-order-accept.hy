// ============================================================================
// defer-lifo-order-accept — Defer executes in LIFO order (last deferred,
// first executed). Multiple defers must all fire in reverse registration
// order. This pins the ordering contract for the emitter.
// ---- Expected verdict: ACCEPT (builds and prints 3 2 1). ----
// EXPECTED-VERDICT: ACCEPT

fn main() {
    defer fmt.println("3")
    defer fmt.println("2")
    defer fmt.println("1")
}
