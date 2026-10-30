// ============================================================================
// closure-reassign-captured-move-reject -- issue #301
//
// PURPOSE: pin the closure reassignment bypass. Inside a closure, `alias` is
// seeded fresh (`alias := map[...]{"k": 1}`) and then REASSIGNED to the
// captured outer binding (`alias = m`). The reassignment used to leave no
// origin/alias trail, so a later `task worker(move alias, done)` read as an
// owned move. Landed #206 (process_closure_assign) closed the gap; four
// companion PRs (#302/#311/#479/...) were closed as same-fix. This file pins
// the repro so the seam cannot silently reopen.
//
// ---- Tier-0 rule set applied to each value ----
// Layer 1 (Go-side GC): `m` is a mutable map owned by main and captured by
//   the closure; the outer scope retains it and the closure runs twice, so
//   `alias` never becomes exclusively owned (O5.1 / O5.4). `move alias` at the
//   task boundary is therefore a non-owned move.
// Layer 2/3: none.
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: REJECT A1b
//   REJECT -- A1b/E3004 (move requires an exclusively owned local binding).
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

fn worker(m map[string]int, done chan<- bool) {
    for i in 0..1000 {
        m["k"] = i
    }
    done <- true
}

fn main() {
    m := map[string]int{"k": 0}
    done := make(chan bool, 2)

    spawn_it := fn() {
        alias := map[string]int{"k": 1}
        alias = m
        task worker(move alias, done)
    }

    spawn_it()
    spawn_it()
    <-done
    <-done
}
