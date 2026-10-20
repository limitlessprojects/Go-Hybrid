// ============================================================================
// 59-rebind-forgets-defer-capture — Checker corpus #59 (issue #438)
//
// PURPOSE: rebinding a NAME must not forget what a live `defer` captured.
// `defer report(shared)` evaluates its argument at the defer statement, so it
// holds the map that `shared` named THEN. Rebinding `shared` to a fresh map
// does not change that, and `alias` still holds the captured storage — so
// moving `alias` must still be seen as racing the deferred read.
//
// This is a REGRESSION case: the program rejected on 8dd0002 and was accepted
// on c53978b.
//
// ---- Tier-0 rule set applied to each value ----
// Layer 1 (Go-side GC): `alias` and the deferred argument name the same map.
//   `move alias` transfers it; the deferred read at function exit is a use
//   after that transfer (O5.2).
// Layer 2/3: none.
//
// ---- Sendability / check sites ----
//   [S1] `task worker(move alias)` — the transfer of the captured storage.
//   [S2] the deferred `report(shared)` — a read of it at function exit.
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: REJECT A1b
//   REJECT — A1b/HYB-E3003. Accepting this is a P0: the emitted Go keeps
//   `defer report(shared)` and `go worker(alias)` and they race
//   (`fatal error: concurrent map read and map write`, reproduced 5/5).
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

fn report(m map[string]int) int {
    return m["task"]
}

fn worker(m map[string]int) {
    m["task"]++
}

fn main() {
    shared := map[string]int{}
    alias := shared

    defer report(shared)            // captures the CURRENT map

    shared = map[string]int{}       // rebind: the NAME moves on, the value does not

    task worker(move alias)         // [S1] transfers the captured storage
}
