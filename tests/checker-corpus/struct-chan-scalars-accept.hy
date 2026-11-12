// ============================================================================
// struct-chan-scalars-accept — Checker corpus (bounty track: "grow the
// adversarial corpus").
//
// PURPOSE: the ACCEPT-side control for struct-chan-shared-map-reject. A
//   struct carrying a CHANNEL plus immutable scalar fields is move-sent to a
//   task. Deep sendability must accept it: the channel is safe-by-type, the
//   string/int fields are immutable/copied, and `move` transfers the value.
//   This guards against a fix that rejects every struct containing a channel.
//
// RULE CITATION: A1c route 1 (safe-by-type channel) + immutable scalars,
//   plus an owned `move` of the struct. No field aliases mutable GC storage.
//
// ---- Tier-0 rule set applied ----
//   Layer 1 (Go-side GC): `p` (packet value, moved), `p.done` (channel),
//     `p.label` (string — immutable), `p.count` (int — copied), `done` /
//     `finished` (channels).
//   Layer 2 (Rust-side owned): none.
//   Layer 3 (boundary handles): none.
//
// ---- Sendability / check sites ----
//   [S1] `task worker(move p, finished)` — p's fields are all sendable and
//        the binding is moved; the sender never uses p again. ACCEPT.
//   [S2] `p.done <- true` — channel send inside the worker; legal.
//   [S3] `finished <- true` — separate channel; legal.
//
// ---- Expected verdict (hand-derived from VERDICTS.md A1c BEFORE running
//      the checker) ----
// EXPECTED-VERDICT: ACCEPT
//   ACCEPT — 0 annotations, 0 managed. A checker that rejects this is
//   over-strict on move-sent struct values with channel fields (bug in the
//   checker, not the corpus).
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

struct packet {
    label string
    count int
    done  chan bool
}

fn worker(p packet, finished chan<- bool) {
    fmt.println("label={p.label} count={p.count}")
    p.done <- true
    finished <- true
}

fn main() {
    done := make(chan bool, 1)
    finished := make(chan bool, 1)
    p := packet{label: "ok", count: 7, done: done}
    task worker(move p, finished)   // SPAWN SITE [S1] — owned move; all fields sendable
    <-finished
}
