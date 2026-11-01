// ============================================================================
// call-return-subslice-freeze — additive registration 2026-11-01 (bounty track).
//
// PURPOSE: a slice is a header over a backing array, and a function that
// returns `xs[1:]` returns a header aliasing the caller's element storage.
// Call-return alias tracking must cover derived slice views, not only map
// headers: after the original slice is move-transferred, an element write
// through the returned view is use-after-move on the same backing array.
//
// ---- Sendability / check sites ----
//   [S1] `view := tail(xs)` — the returned sub-slice aliases `xs`'s array.
//   [S2] `task worker(move xs, done)` — owned move of the slice binding; the
//        freeze covers the array and every live view of it.
//   [S3] `view[0] = 9` — element write through the live view after the move.
//
// ---- Expected verdict (derived from the call-return alias edge + O5.2) ----
// EXPECTED-VERDICT: REJECT A1b
// ============================================================================

fn tail(xs []int) []int {
    return xs[1:]
}

fn worker(xs []int, done chan<- bool) {
    xs[0] = 1
    done <- true
}

fn main() {
    xs := []int{1, 2, 3}
    view := tail(xs)
    done := make(chan bool, 1)
    task worker(move xs, done)
    view[0] = 9
    <-done
}
