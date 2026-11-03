// ============================================================================
// closure-struct-capture-freeze — additive registration 2026-11-01 (bounty track).
//
// PURPOSE: a closure stored in a struct field is still a closure: it captures
// its environment, and the capture binding is linked to every captured value
// (corpus 36). Moving the container must therefore freeze the captures too —
// a struct field is not a laundering path for the capture edge.
//
// ---- Sendability / check sites ----
//   [S1] `task worker(move h, done)` — owned move of the struct whose `f`
//        field captures `shared`; the freeze reaches the captured map header.
//   [S2] `shared["main"]++` — write through the live capture after the move.
//
// ---- Expected verdict (derived from the closure-capture link + O5.2) ----
// EXPECTED-VERDICT: REJECT A1b
// ============================================================================

struct holder {
    f fn()
}

fn worker(h holder, done chan<- bool) {
    h.f()
    done <- true
}

fn main() {
    shared := map[string]int{}
    h := holder{f: fn() { shared["task"]++ }}
    done := make(chan bool, 1)
    task worker(move h, done)
    shared["main"]++
    <-done
}
