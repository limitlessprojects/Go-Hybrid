// ============================================================================
// 60-rebind-without-defer-control — Checker corpus #60 (issue #438)
//
// PURPOSE: the ACCEPT control for #59. A rebind on its own is ordinary code —
// no `defer` captures the old map and nothing is moved across a boundary, so
// the program must stay accepted. A checker that rejects this is over-strict.
//
// ---- Tier-0 rule set applied to each value ----
// Layer 1 (Go-side GC): `alias` keeps the old map, `shared` names a new one;
//   both stay inside one task, so nothing is shared across a boundary.
// Layer 2/3: none.
//
// ---- Sendability / check sites ----
//   none — no spawn, no send.
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: ACCEPT
//   ACCEPT — 0 annotations, 0 managed.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

fn main() {
    shared := map[string]int{}
    alias := shared

    shared = map[string]int{}

    alias["main"]++
    fmt.println("{len(shared)} {len(alias)}")
}
