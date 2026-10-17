// ============================================================================
// 54-defer-live-reference-at-spawn — Checker corpus #54 (issue #415)
//
// PURPOSE: negative control for the deferred-call gap. `defer` evaluates its
// arguments at the defer SITE but calls at function EXIT. The checker treated
// the arguments as ordinary reads at the defer statement, so a later `move` of
// the same binding looked legal — the deferred call then reads the map while
// the spawned task writes it.
//
// ---- Tier-0 rule set applied to each value ----
// Layer 1 (Go-side GC): `shared` is a mutable map. `defer report(shared)`
//   captures it, and the CALL happens after `task worker(move shared)`
//   transferred ownership. The deferred read therefore happens after the
//   move — O5.2 forbids it.
// Layer 2/3: none.
//
// ---- Sendability / check sites ----
//   [S1] `task worker(move shared)` — the transfer.
//   [S2] the deferred `report(shared)` — a use AFTER that transfer.
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: REJECT A1b
//   REJECT — A1b/HYB-E3003 ("moved into a task while a live `defer` still
//   reads it"). Accepting this program is a P0: the emitted Go runs
//   `defer report(shared)` and `go worker(shared)` in the same function and
//   races — `fatal error: concurrent map read and map write` (reproduced 5/5).
//
//   NOTE for reproducers: `len(m)` inside the deferred call does NOT
//   reproduce — `len` reads the map header, not the map, so the runtime
//   detector stays quiet. The deferred call must index the map.
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
    defer report(shared)            // call happens at function exit

    task worker(move shared)        // [S1] ownership leaves before that call
}
