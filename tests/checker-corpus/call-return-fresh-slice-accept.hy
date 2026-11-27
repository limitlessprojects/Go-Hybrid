// ============================================================================
// call-return-fresh-slice-accept — ACCEPT-side control for
// call-return-subslice-freeze (corpus GROWTH, bounty track).
//
// WHY THIS EXISTS: the reject case pins that `tail(xs)` returns a view over the
// caller's array. This control keeps the call-return-of-a-slice shape, the
// `view := ...` binding, the `move xs` spawn and the `view[0]` element write,
// and changes only the callee body: it allocates its own slice, so the returned
// header does not alias `xs`.
//
// EXPECTED-VERDICT: ACCEPT
// ============================================================================

fn fresh(n int) []int {
    return []int{n, n + 1, n + 2}
}

fn worker(xs []int, done chan<- bool) {
    xs[0] = 1
    done <- true
}

fn main() {
    xs := []int{1, 2, 3}
    view := fresh(7)
    done := make(chan bool, 1)
    task worker(move xs, done)
    view[0] = 9
    <-done
}
