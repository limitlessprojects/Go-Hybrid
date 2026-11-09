// ============================================================================
// struct-chan-slice-send-reject — Checker corpus (bounty track: "grow the
// adversarial corpus").
//
// PURPOSE: the SLICE counterpart of struct-chan-shared-map-send-reject. A
//   struct carrying a CHANNEL and a live SLICE field is moved over a channel
//   while the sender still holds the slice binding. The move of the struct
//   does not transfer the slice's backing array away from the sender; the
//   sender's later write is a use-after-move through a live alias, and the
//   freeze must reach the slice field on the send path, not only a map field.
//
// RULE CITATION: A1b + A4: `p` is a value copy of a struct whose `xs` field
//   carries the same backing-array pointer as `xs`; `packets <- move p` freezes
//   that storage, so `xs[0] = 7` after the send is a write through a live alias
//   of a moved value.
//
// ---- Tier-0 rule set applied ----
//   Layer 1 (Go-side GC): `p` (packet value, moved), `p.xs` (slice header copy),
//     `p.done` (channel), `xs` (the original slice binding, aliased).
//   Layer 2 (Rust-side owned): none.
//   Layer 3 (boundary handles): none.
//
// ---- Sendability / check sites ----
//   [S1] `packets <- move p` — the struct is moved, but its slice field aliases
//        `xs`; the move freezes that backing array.
//   [S2] `xs[0] = 7` — write through the live alias after the freeze.
//        VIOLATION (A1b).
//
// ---- Expected verdict (hand-derived from VERDICTS.md A1b + A4 BEFORE
//      running the checker) ----
// EXPECTED-VERDICT: REJECT A1b
// EXPECTED-DIAGNOSTIC: HYB-E3003
//   REJECT — A1b. Moving the struct over the channel does not transfer the
//   slice storage away from the sender's `xs` binding; the later write is
//   use-after-move through a live alias.
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
    p.xs[0] = 9
    p.done <- true
}

fn main() {
    xs := []int{1, 2, 3}
    done := make(chan bool, 1)
    packets := make(chan packet, 1)
    p := packet{xs: xs, done: done}
    task consumer(packets)
    packets <- move p     // move freezes the aliased slice storage
    xs[0] = 7             // [S2] write through a live alias — A1b
    <-done
}
