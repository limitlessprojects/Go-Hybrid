// ============================================================================
// any-erasure-shared-map-at-spawn — Checker corpus (bounty track: "grow the adversarial corpus").
//
// PURPOSE: TYPE ERASURE as a laundering route. `any` appears in 27/60 files, but never
//   as a binding that erases a shared mutable reference on its way to a
//   boundary. `var v any = m` is the shape that would defeat a checker that
//   classified the `any` keyword instead of the value behind it.
//
// RULE CITATION: A1c (safe-by-type) at the spawn site. Erasing the static type does not change
//   what the value IS: `v` still names the same mutable map, so it is not
//   channel/atomic/immutable, not `move`d, not `managed` — not sendable.
//
// ---- Tier-0 rule set applied ----
//   Layer 1 (Go-side GC): `m` (map[string]int), `v` (any — the erased alias),
//     `done` (channel).
// ---- Sendability / check sites ----
//   [S1] `task worker(v, done)` — VIOLATION: the argument is `any`, but the
//        value behind it is the live map `m`.
//   [S2] `m["a"] = 2` — the sender writes the map the task body can reach.
// ---- Expected verdict (hand-derived from VERDICTS.md rule key + SPEC O4
//      BEFORE running the checker) ----
// EXPECTED-VERDICT: REJECT A1c
// EXPECTED-DIAGNOSTIC: HYB-E3001
//   REJECT — A1c. The paired ACCEPT control (any-erasure-primitive-control) shows
//   the rule tracks the VALUE: an `any` holding an immutable primitive is
//   accepted, so this rejection is about the map, not the `any` keyword.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

fn worker(v any, done chan<- bool) {
    fmt.println("got")
    done <- true
}

fn main() {
    m := map[string]int{"a": 1}
    var v any = m          // type erasure at the binding, not at the boundary
    done := make(chan bool, 1)
    task worker(v, done)   // [S1] VIOLATION: `any` must not launder the shared map
    m["a"] = 2             // [S2] the sender writes what the task can reach
    <-done
}
