// ============================================================================
// chan-slice-aliased-at-send — Checker corpus (bounty track: "grow the adversarial corpus").
//
// PURPOSE: the SEND-side twin of the spawn-side element work. The corpus pins channel
//   sends of plain maps (16/17, 32-select-send) and spawns of aliased slices
//   (45/53), but never a channel whose ELEMENT TYPE is a slice: `chan []int`
//   is 0/60 today. A slice value carries a backing-array pointer, so sending
//   one without `move` while the sender keeps writing it is the same
//   unsynchronized-sharing shape A1c exists to reject.
//
// RULE CITATION: A1c, "sendability, safe-by-type" (VERDICTS.md rule key; SPEC-LANGUAGE.md O4,
//   §9.3). A slice is not a channel/atomic/immutable value, is not `move`d and
//   is not inside `managed` — not sendable.
//
// ---- Tier-0 rule set applied ----
//   Layer 1 (Go-side GC): `shared` ([]int — the sent value's backing array),
//     `ch` (chan []int), `xs` (the receiver's copy of the header).
//   Layer 2 (Rust-side owned): none.
// ---- Sendability / check sites ----
//   [S1] `ch <- shared` — VIOLATION: `shared` is a live mutable slice and the
//        send is not a `move`. `xs[0]` in the receiver and `shared[0]` in the
//        sender name the same array element — a data race on `[]int`.
// ---- Expected verdict (hand-derived from VERDICTS.md rule key + SPEC O4
//      BEFORE running the checker) ----
// EXPECTED-VERDICT: REJECT A1c
// EXPECTED-DIAGNOSTIC: HYB-E3002
//   REJECT — A1c. The slice's backing array is shared, so the receiver's read and
//   the sender's write are unsynchronized. Legal routes: `ch <- move shared`
//   when the sender is done, a fresh literal, or a per-task copy merged back.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

fn consumer(ch <-chan []int) {
    xs := <-ch
    fmt.println("n: {len(xs)}")
}

fn main() {
    shared := []int{1, 2, 3}
    ch := make(chan []int, 1)
    task consumer(ch)
    ch <- shared          // [S1] VIOLATION: live mutable slice sent without `move`
    shared[0] = 9         // the sender keeps writing the array the receiver holds
    fmt.println("done")
}
