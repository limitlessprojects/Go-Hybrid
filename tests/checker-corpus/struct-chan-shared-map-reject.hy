// ============================================================================
// struct-chan-shared-map-reject — Checker corpus (bounty track: "grow the
// adversarial corpus").
//
// PURPOSE: a STRUCT carrying a CHANNEL and a SHARED MAP field is move-sent to
//   a task while the sender still holds an alias of the map. The channel field
//   is safe-by-type, but the map header is a reference: moving the struct must
//   not launder the sender's live alias. The corpus covers struct fields of
//   slice/map type and channel values passed directly, but no program puts a
//   channel and a shared map in the same struct and move-sends it.
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
//   [S1] `task worker(move p, finished)` — the struct is moved, but its map
//        field aliases `m`; the move freezes the aliased storage.
//   [S2] `m["after"] = 2` — write through the live alias after the freeze.
//        VIOLATION (A1b).
//
// ---- Expected verdict (hand-derived from VERDICTS.md A1b + A4 BEFORE
//      running the checker) ----
// EXPECTED-VERDICT: REJECT A1b
//   REJECT — A1b. Moving the struct does not transfer the map storage away
//   from the sender's `m` binding; the later write is use-after-move through
//   a live alias.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

struct packet {
    shared map[string]int
    done   chan bool
}

fn worker(p packet, finished chan<- bool) {
    p.shared["seen"] = 1
    p.done <- true
    finished <- true
}

fn main() {
    m := map[string]int{"a": 1}
    done := make(chan bool, 1)
    finished := make(chan bool, 1)
    p := packet{shared: m, done: done}
    task worker(move p, finished)   // move freezes the aliased map storage
    m["after"] = 2                  // [S2] write through a live alias — A1b
    <-finished
}
