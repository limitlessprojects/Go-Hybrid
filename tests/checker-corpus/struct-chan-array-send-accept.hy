// ============================================================================
// struct-chan-array-send-accept — Checker corpus (bounty track: "grow the
// adversarial corpus").
//
// PURPOSE: contrast fixed-size value arrays `[N]int` against slice headers
//   `[]int` inside a struct moved over a channel (`chan packet`). A `[2]int`
//   array has value semantics in Go/Hybrid: embedding `arr` into `packet`
//   copies the elements, so moving `p` over `packets` leaves `arr` independent
//   and legal to mutate in the sender afterwards.
//
// RULE CITATION: A1c route 1 (safe-by-type channel) + fixed-size scalar array
//   value copy, plus an owned `move` of the struct at the channel send site.
//
// ---- Tier-0 rule set applied ----
//   Layer 1 (Go-side GC): `p` (packet value, moved), `p.done` (channel),
//     `p.pair` (`[2]int` — copied by value), `arr` (independent local `[2]int`),
//     `done` / `packets` (channels).
//   Layer 2 (Rust-side owned): none.
//   Layer 3 (boundary handles): none.
//
// ---- Sendability / check sites ----
//   [S1] `packets <- move p` — p's fields are all sendable (`[2]int` + `chan bool`)
//        and `p` is moved; `arr` was copied by value into `p.pair`. ACCEPT.
//   [S2] `arr[0] = 9` — write to the sender's independent value array; legal.
//
// ---- Expected verdict (hand-derived from VERDICTS.md A1c BEFORE running
//      the checker) ----
// EXPECTED-VERDICT: ACCEPT
//   ACCEPT — 0 annotations, 0 managed. A checker that rejects this confuses
//   value-copied `[N]T` arrays with reference-aliasing `[]T` slices.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

struct packet {
    pair [2]int
    done chan bool
}

fn consumer(packets <-chan packet) {
    p := <-packets
    fmt.println("v={p.pair[0]}")
    p.done <- true
}

fn main() {
    arr := [2]int{1, 2}
    done := make(chan bool, 1)
    packets := make(chan packet, 1)
    p := packet{pair: arr, done: done}
    task consumer(packets)
    packets <- move p     // S1 — owned move; [2]int was copied by value
    arr[0] = 9            // S2 — arr is an independent value array
    fmt.println("local={arr[0]}")
    <-done
}
