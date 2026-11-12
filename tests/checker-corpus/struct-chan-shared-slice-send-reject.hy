// ============================================================================
// struct-chan-shared-slice-send-reject — Checker corpus (bounty track: "grow
// the adversarial corpus").
//
// PURPOSE: the slice-alias companion of struct-chan-shared-map-send-reject. A
//   struct carrying a CHANNEL and a SLICE field (`[]int`) is moved over a
//   channel while the sender still holds an alias `s` of the slice's backing
//   array. Moving the struct over the channel must freeze `s`, so a subsequent
//   indexed write `s[0] = 9` is rejected as a write through a live alias of a
//   moved value (A1b / HYB-E3003).
//
// RULE CITATION: A1b + A4: `p` embeds `s` (`[]int`), which copies the slice
//   header but shares the underlying array. Moving `p` at `packets <- move p`
//   freezes the aliased storage, so `s[0] = 9` violates A1b.
//
// ---- Tier-0 rule set applied ----
//   Layer 1 (Go-side GC): `p` (packet value, moved), `p.items` (slice header
//     copy), `p.done` (channel), `s` (original slice binding, aliased).
//   Layer 2 (Rust-side owned): none.
//   Layer 3 (boundary handles): none.
//
// ---- Sendability / check sites ----
//   [S1] `packets <- move p` — moves the struct, freezing the aliased slice `s`.
//   [S2] `s[0] = 9` — write through a live alias after the freeze.
//        VIOLATION (A1b).
//
// ---- Expected verdict (hand-derived from VERDICTS.md A1b + A4 BEFORE
//      running the checker) ----
// EXPECTED-VERDICT: REJECT A1b
//   REJECT — A1b. Moving the struct over the channel does not unshare the
//   slice's backing array from `s`; the later element write is use-after-move
//   through a live alias.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

struct packet {
    items []int
    done  chan bool
}

fn consumer(packets <-chan packet) {
    p := <-packets
    p.items[0] = 1
    p.done <- true
}

fn main() {
    s := []int{1, 2}
    done := make(chan bool, 1)
    packets := make(chan packet, 1)
    p := packet{items: s, done: done}
    task consumer(packets)
    packets <- move p     // move freezes the aliased slice backing storage
    s[0] = 9              // [S2] write through a live alias — A1b
    <-done
}
