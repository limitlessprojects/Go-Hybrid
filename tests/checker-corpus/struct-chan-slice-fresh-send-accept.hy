// ============================================================================
// struct-chan-slice-fresh-send-accept — Checker corpus (bounty track: "grow
// the adversarial corpus").
//
// PURPOSE: the ACCEPT control for struct-chan-slice-send-reject. The same
//   struct shape carries a channel and a SLICE field, and it is moved over a
//   channel, but the slice is a fresh composite literal at the send site:
//   nothing else names its backing array, so there is no alias to freeze and
//   no unsynchronized sharing. A checker that rejects this would ban slices
//   inside moved structs outright, which is far too strong.
//
// RULE CITATION: A1b does not apply: the sent slice has no other binding and
//   the sender never writes it. The move transfers the struct value; the
//   receiver owns the fresh slice.
//
// ---- Tier-0 rule set applied ----
//   Layer 1 (Go-side GC): `p` (packet value, moved), `p.xs` (fresh slice
//     literal, no other binding), `p.done` (channel).
//   Layer 2 (Rust-side owned): none.
//   Layer 3 (boundary handles): none.
//
// ---- Sendability / check sites ----
//   [S1] `packets <- move p` — the struct is moved; the slice field is fresh;
//        the channel field is safe-by-type. ACCEPT.
//   [S2] `p.done <- true` — channel send inside the consumer; legal.
//
// ---- Expected verdict (hand-derived from VERDICTS.md A1b BEFORE running
//      the checker) ----
// EXPECTED-VERDICT: ACCEPT
//   ACCEPT — 0 annotations, 0 managed. Pins that the rejection above tracks
//   the ALIAS, not the `[]int` field type or the channel-send path.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

struct packet {
    xs   []int
    done chan bool
}

fn consumer(packets <-chan packet) {
    p := <-packets
    fmt.println("n: {len(p.xs)}")
    p.done <- true
}

fn main() {
    done := make(chan bool, 1)
    packets := make(chan packet, 1)
    p := packet{xs: []int{1, 2, 3}, done: done}
    task consumer(packets)
    packets <- move p     // [S1] fresh slice literal; nothing else names the array
    <-done
}
