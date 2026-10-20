// ============================================================================
// 56-defer-live-reference-via-alias — Checker corpus #56 (issue #415)
//
// PURPOSE: the deferred binding does not have to be the binding that is moved.
// `defer report(alias)` names the ALIAS; `task worker(move shared)` transfers
// the ORIGIN. Both name one map, so the deferred read at function exit still
// happens after ownership left.
//
// ---- Tier-0 rule set applied to each value ----
// Layer 1 (Go-side GC): `alias := shared` makes the two bindings one map.
//   The deferred call reads through `alias` at exit; ownership of `shared`
//   left at the spawn. O5.2 freezes every live alias, not just the moved name.
// Layer 2/3: none.
//
// ---- Sendability / check sites ----
//   [S1] `task worker(move shared)` — the transfer of the origin.
//   [S2] the deferred `report(alias)` — a use through the alias, AFTER [S1].
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: REJECT A1b
//   REJECT — A1b/HYB-E3003. Keying the check on the moved NAME alone misses
//   this shape; it must key on the alias/may-share component.
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

    defer report(alias)             // the deferred call names the ALIAS

    task worker(move shared)        // [S1] ownership of the ORIGIN leaves
}
