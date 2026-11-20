// ============================================================================
// map-elem-struct-slice-fresh-spawn-accept — Checker corpus (bounty track:
// "grow the adversarial corpus").
//
// PURPOSE: the ACCEPT control for map-elem-struct-slice-spawn-reject. The same
//   map-of-struct shape carries a SLICE field, and the map is moved to a task,
//   but the slice is a fresh composite literal inside the map literal: nothing
//   else names its backing array, so there is no alias to freeze and no
//   unsynchronized sharing. A checker that rejects a slice field in a map
//   value outright would be far too strong — this pins the alias, not the type.
//
// RULE CITATION: A1b does not apply: the stored slice has no other binding and
//   the sender never touches the map or the array again. The move transfers
//   the map value; the receiver owns the fresh element storage.
//
// ---- Tier-0 rule set applied ----
//   Layer 1 (Go-side GC): `m` (map[string]holder — moved), `m["a"].xs`
//     (fresh []int literal, no other binding), `done` (channel).
//   Layer 2 (Rust-side owned): none.
//   Layer 3 (boundary handles): none.
//
// ---- Sendability / check sites ----
//   [S1] `map[string]holder{"a": holder{xs: []int{1, 2, 3}}}` — the element
//        store carries a fresh slice header; no second binding exists.
//   [S2] `task worker(move m, done)` — owned move of the map. ACCEPT.
//   [S3] `m["a"].xs[0]` inside the worker — the receiver reads its own array.
//
// ---- Expected verdict (hand-derived from VERDICTS.md A1b + A4 BEFORE
//      running the checker) ----
// EXPECTED-VERDICT: ACCEPT
//   ACCEPT — 0 annotations, 0 managed. Pins that the rejection above tracks
//   the ALIAS through the map element, not the `[]int` field type, the map
//   literal, or the spawn path.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

struct holder {
    xs []int
}

fn worker(m map[string]holder, done chan<- bool) {
    fmt.println("first: {m["a"].xs[0]}")
    done <- true
}

fn main() {
    m := map[string]holder{"a": holder{xs: []int{1, 2, 3}}}   // [S1] fresh slice, no alias
    done := make(chan bool, 1)
    task worker(move m, done)                                  // [S2] owned move — ACCEPT
    <-done
}
