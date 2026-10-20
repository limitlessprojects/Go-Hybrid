// ============================================================================
// map-elem-fresh-map-control — Checker corpus (bounty track: "grow the adversarial corpus").
//
// PURPOSE: the ACCEPT control for map-elem-map-alias-at-spawn. Identical program with
//   ONE change: the write after the move targets a FRESH map that never came
//   out of `m`, so no alias of the moved storage is touched. A checker that
//   rejects this is over-strict — it would ban every map local in a function
//   that later moves an unrelated map.
//
// RULE CITATION: A1b does not apply: the moved value's storage is never reached again;
//   A4 makes the fresh literal a new binding with no link to `m`.
//
// ---- Tier-0 rule set applied ----
//   Layer 1 (Go-side GC): `m` (moved), `fresh` (an independent map), `done`.
//   Layer 2 (Rust-side owned): none.

// ---- Sendability / check sites ----
//   [S1] `task worker(move m, done)` — legal owned move.
//   [S2] `fresh["z"] = 9` — an ordinary write to storage no one else holds.

// ---- Expected verdict (hand-derived from VERDICTS.md rule key + SPEC
//      O3/O4/O9 BEFORE running the checker) ----
// EXPECTED-VERDICT: ACCEPT
//   ACCEPT — 0 annotations, 0 managed. Pins that the REJECT above tracks the
//   ALIAS, not the presence of a map local near a move.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

struct env {
    services map[string]map[string]int
}

fn worker(services map[string]map[string]int, done chan<- bool) {
    fmt.println("frontend replicas: {services["frontend"]["replicas"]}")
    done <- true
}

fn main() {
    e := env{services: map[string]map[string]int{"frontend": map[string]int{"replicas": 2}}}
    m := e.services
    fresh := map[string]int{"z": 5}   // independent storage: no link to `m`
    done := make(chan bool, 1)
    task worker(move m, done)
    fresh["z"] = 9                    // legal: nobody else holds this map
    <-done
}
