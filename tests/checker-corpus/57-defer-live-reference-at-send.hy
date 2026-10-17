// ============================================================================
// 55-defer-live-reference-at-send — Checker corpus #55 (issue #415)
//
// PURPOSE: the SEND-side twin of #54. The same deferred-call gap exists at a
// channel send, not just at a spawn: `ch <- move shared` transfers ownership
// on the spot, and the deferred call reads `shared` afterwards at function
// exit.
//
// ---- Tier-0 rule set applied to each value ----
// Layer 1 (Go-side GC): `shared` is a mutable map. `defer report(shared)`
//   captures it and CALLS at function exit, after `ch <- move shared`
//   transferred ownership. The deferred read is therefore a use-after-move.
// Layer 2/3: none.
//
// ---- Sendability / check sites ----
//   [S1] `ch <- move shared` — the transfer.
//   [S2] the deferred `report(shared)` — a use AFTER that transfer.
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: REJECT A1b
//   REJECT — A1b/HYB-E3003 ("moved into a channel while a live `defer` still
//   reads it"). The emitted Go keeps `defer report(shared)` and sends the map
//   to a consumer; the deferred read races the consumer's writes.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

fn report(m map[string]int) int {
    return m["task"]
}

fn main() {
    shared := map[string]int{}
    ch := make(chan map[string]int, 1)
    defer report(shared)            // call happens at function exit

    ch <- move shared               // [S1] ownership leaves before that call
}
