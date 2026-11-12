// ============================================================================
// struct-chan-shared-map-send-reject — Checker corpus (bounty track: "grow the
// adversarial corpus").
//
// PURPOSE: the SEND-SITE companion of struct-chan-shared-map-reject. A struct
//   carrying a CHANNEL and a SHARED MAP field is moved over a channel while
//   the sender still holds an alias of the map. The channel field is
//   safe-by-type, but the map header is a reference: moving the struct must
//   not launder the sender's live alias, and the freeze must reach the map
//   field on the send path, not just the spawn path.
//
// RULE CITATION: A1b + A4: `p` is a value copy of a struct whose `shared`
//   field is a map header; `m` aliases the same map. Moving `p` freezes the
//   map the sender still holds, so the later `m["after"] = 2` is a write
//   through a live alias of a moved value.
//
// ---- Tier-0 rule set applied ----
//   Layer 1 (Go-side GC): `p` (packet value, moved), `p.shared` (map header
//     copy), `p.done` (channel), `m` (the original map binding, aliased).
//   Layer 2 (Rust-side owned): none.
//   Layer 3 (boundary handles): none.
//
// ---- Sendability / check sites ----
//   [S1] `packets <- move p` — the struct is moved, but its map field aliases
//        `m`; the move freezes the aliased storage.
//   [S2] `m["after"] = 2` — write through the live alias after the freeze.
//        VIOLATION (A1b).
//
// ---- Expected verdict (hand-derived from VERDICTS.md A1b + A4 BEFORE
//      running the checker) ----
// EXPECTED-VERDICT: REJECT A1b
//   REJECT — A1b. Moving the struct over the channel does not transfer the
//   map storage away from the sender's `m` binding; the later write is
//   use-after-move through a live alias.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

struct packet {
    shared map[string]int
    done   chan bool
}

fn consumer(packets <-chan packet) {
    p := <-packets
    p.shared["seen"] = 1
    p.done <- true
}

fn main() {
    m := map[string]int{"a": 1}
    done := make(chan bool, 1)
    packets := make(chan packet, 1)
    p := packet{shared: m, done: done}
    task consumer(packets)
    packets <- move p     // move freezes the aliased map storage
    m["after"] = 2        // [S2] write through a live alias — A1b
    <-done
}
