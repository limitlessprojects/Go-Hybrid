// ============================================================================
// struct-chan-scalars-send-accept — Checker corpus (bounty track: "grow the
// adversarial corpus").
//
// PURPOSE: the SEND-SITE companion of struct-chan-scalars-accept. A struct
//   carrying a CHANNEL plus immutable scalar fields is moved over a channel
//   (`chan packet`), not spawned. Deep sendability must accept it: the channel
//   field is safe-by-type, the string/int fields are immutable/copied, and
//   `move` transfers the struct. This guards the send path against a fix that
//   only teaches the spawn path about struct values.
//
// RULE CITATION: A1c route 1 (safe-by-type channel) + immutable scalars,
//   plus an owned `move` of the struct at the send site.
//
// ---- Tier-0 rule set applied ----
//   Layer 1 (Go-side GC): `p` (packet value, moved), `p.done` (channel),
//     `p.label` (string — immutable), `p.count` (int — copied), `done` /
//     `packets` (channels).
//   Layer 2 (Rust-side owned): none.
//   Layer 3 (boundary handles): none.
//
// ---- Sendability / check sites ----
//   [S1] `packets <- move p` — p's fields are all sendable and the binding is
//        moved; the sender never uses p again. ACCEPT.
//   [S2] `p.done <- true` — channel send inside the consumer; legal.
//
// ---- Expected verdict (hand-derived from VERDICTS.md A1c BEFORE running
//      the checker) ----
// EXPECTED-VERDICT: ACCEPT
//   ACCEPT — 0 annotations, 0 managed. A checker that rejects this is
//   over-strict on move-sent struct values at a send site (bug in the checker,
//   not the corpus).
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

struct packet {
    label string
    count int
    done  chan bool
}

fn consumer(packets <-chan packet) {
    p := <-packets
    fmt.println("label={p.label} count={p.count}")
    p.done <- true
}

fn main() {
    done := make(chan bool, 1)
    packets := make(chan packet, 1)
    p := packet{label: "ok", count: 7, done: done}
    task consumer(packets)
    packets <- move p     // S1 — owned move; all fields sendable
    <-done
}
