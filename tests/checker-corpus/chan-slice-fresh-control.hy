// ============================================================================
// chan-slice-fresh-control — Checker corpus (bounty track: "grow the adversarial corpus").
//
// PURPOSE: the ACCEPT control for chan-slice-aliased-at-send. The channel element type
//   is unchanged; the sent value is a fresh literal, so no other binding names
//   its backing array and there is nothing to race with. A checker that
//   rejects this would ban `chan []T` outright, which is far too strong.
//
// RULE CITATION: A1c does not apply: the sent value is fresh storage, and a slice literal is
//   the route the A1c fix-it names ("send a copy").
//
// ---- Tier-0 rule set applied ----
//   Layer 1 (Go-side GC): `ch` (chan []int); the literal's array has no other
//     binding.
// ---- Sendability / check sites ----
//   [S1] `ch <- []int{1, 2, 3}` — a fresh composite literal at the send site:
//        nothing else holds it, so the value is sendable.
// ---- Expected verdict (hand-derived from VERDICTS.md rule key + SPEC O4
//      BEFORE running the checker) ----
// EXPECTED-VERDICT: ACCEPT
//   ACCEPT — 0 annotations, 0 managed. Pins that the REJECT above tracks the
//   ALIAS, not the `chan []int` element type itself.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

fn consumer(ch <-chan []int) {
    xs := <-ch
    fmt.println("n: {len(xs)}")
}

fn main() {
    ch := make(chan []int, 1)
    task consumer(ch)
    ch <- []int{1, 2, 3}   // fresh literal: no other binding names this array
    fmt.println("done")
}
