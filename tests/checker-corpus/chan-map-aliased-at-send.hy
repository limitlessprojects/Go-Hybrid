// ============================================================================
// chan-map-aliased-at-send — Checker corpus (bounty track: "grow the adversarial corpus").
//
// PURPOSE: the map twin of chan-slice-aliased-at-send, and the WRITE-after-send form.
//   The corpus's map-over-channel cases send a value and never touch it again
//   (16, 17); here the sender writes the map after the send, which is the
//   shape that makes the race reachable rather than merely possible.
//
// RULE CITATION: A1c (safe-by-type) at the send site. `m` is a mutable GC map, not `move`d,
//   not `managed` — not sendable.
//
// ---- Tier-0 rule set applied ----
//   Layer 1 (Go-side GC): `m` (map[string]int — the shared hash table),
//     `ch` (chan map[string]int).
// ---- Sendability / check sites ----
//   [S1] `ch <- m` — VIOLATION: live mutable map sent without `move`.
//   [S2] `m["a"] = 2` — the sender's write lands on the same hash table the
//        receiver ranges over: a concurrent map read/write (Go runtime fatal).
// ---- Expected verdict (hand-derived from VERDICTS.md rule key + SPEC O4
//      BEFORE running the checker) ----
// EXPECTED-VERDICT: REJECT A1c
// EXPECTED-DIAGNOSTIC: HYB-E3002
//   REJECT — A1c. Identical argument to the slice form; the send must be a `move`
//   or the value must be a fresh copy.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

fn consumer(ch <-chan map[string]int) {
    m := <-ch
    fmt.println("n: {len(m)}")
}

fn main() {
    m := map[string]int{"a": 1}
    ch := make(chan map[string]int, 1)
    task consumer(ch)
    ch <- m            // [S1] VIOLATION: live mutable map sent without `move`
    m["a"] = 2         // [S2] write into the hash table the receiver ranges over
    fmt.println("done")
}
