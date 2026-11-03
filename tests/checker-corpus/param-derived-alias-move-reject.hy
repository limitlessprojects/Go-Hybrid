// ============================================================================
// param-derived-alias-move-reject — additive registration 2026-11-01 (bounty track).
//
// PURPOSE: `move` requires an exclusively owned local binding (O5.1 / O5.4).
// A local alias derived from a parameter is not exclusive: the caller retains
// its argument across the intra-task call, so freezing the alias cannot reach
// the caller's storage. Spawning `move alias` inside the callee must reject
// instead of silently transferring storage the callee does not own.
//
// ---- Sendability / check sites ----
//   [S1] `alias := m` — local alias of parameter `m`; every storage edge the
//        parameter held remains live in the caller.
//   [S2] `task worker(move alias, done)` — the move target is not exclusively
//        owned (O5.4), so the spawn cannot transfer it.
//
// ---- Expected verdict (derived from O5.1 / O5.4 before running) ----
// EXPECTED-VERDICT: REJECT A1b
// EXPECTED-DIAGNOSTIC: HYB-E3004
// ============================================================================

fn worker(m map[string]int, done chan<- bool) {
    m["task"]++
    done <- true
}

fn relay(m map[string]int, done chan<- bool) {
    alias := m
    task worker(move alias, done)
}

fn main() {
    shared := map[string]int{}
    done := make(chan bool, 1)
    relay(shared, done)
    shared["main"]++
    <-done
}
