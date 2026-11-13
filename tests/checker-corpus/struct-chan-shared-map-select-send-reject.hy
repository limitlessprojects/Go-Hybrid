// ============================================================================
// struct-chan-shared-map-select-send-reject — Checker corpus (bounty track:
// "grow the adversarial corpus").
//
// PURPOSE: the SELECT-SEND companion of struct-chan-shared-map-reject (#4208)
//   and struct-chan-shared-map-send-reject (#4209). A struct carrying a CHANNEL
//   and a SHARED MAP field is moved through a `select` send clause
//   (`packets <- move p: {}`) while the sender still holds an alias `m` of the
//   map. Moving the struct in a `select` send clause must freeze the aliased
//   map storage (SPEC C5 + O5.2 / A1b), so the subsequent `m["after"] = 2` is
//   rejected as use-after-move through a live alias.
//
// RULE CITATION: SPEC C5 + A1b + A4: `p` is a value copy of a struct whose
//   `shared` field is a map header aliasing `m`. Moving `p` in a `select` send
//   clause freezes the aliased map storage, so `m["after"] = 2` violates A1b.
//
// ---- Tier-0 rule set applied ----
//   Layer 1 (Go-side GC): `p` (packet value, moved in select send arm),
//     `p.shared` (map header copy), `p.done` (channel), `m` (aliased map).
//   Layer 2 (Rust-side owned): none.
//   Layer 3 (boundary handles): none.
//
// ---- Sendability / check sites ----
//   [S1] `packets <- move p: {}` — the struct is moved in a select send clause,
//        freezing the aliased map storage `m`.
//   [S2] `m["after"] = 2` — write through the live alias after the freeze.
//        VIOLATION (A1b).
//
// ---- Expected verdict (hand-derived from VERDICTS.md A1b + A4 + C5 BEFORE
//      running the checker) ----
// EXPECTED-VERDICT: REJECT A1b
//   REJECT — A1b. Moving the struct in a `select` send clause does not transfer
//   the map storage away from the sender's `m` binding; the later write is
//   use-after-move through a live alias.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
//
// ---- Detailed Step-by-Step Alias Freeze & Select-Arm Trace (SPEC C5 + A1b) ----
//   Step 1: m := map[string]int{...} allocates a mutable Go map header
//           in the sender frame; m holds a live reference to the backing map.
//   Step 2: done := make(chan bool, 1) and packets := make(chan packet, 1)
//           initialize the synchronization and packet transport channels.
//   Step 3: p := packet{shared: m, done: done} copies the map header m
//           into field p.shared, creating an intra-thread alias pair
//           (m <-> p.shared) pointing to the same underlying map storage.
//   Step 4: 	ask consumer(packets) launches the concurrent consumer task
//           which mutates p.shared upon receiving p from packets.
//   Step 5: select { packets <- move p: {} } executes a SelectArmKind::Send
//           boundary transfer (SPEC C5):
//             (a) moving p across the channel boundary transfers the struct;
//             (b) because p.shared aliases m, O5.2 / A1b freezes the
//                 aliased map storage reachable through m in the sender;
//             (c) any subsequent mutation or read through m in the sender
//                 would race with the concurrent mutation inside consumer.
//   Step 6: m[...] = 2 attempts to mutate the frozen map alias m
//           after the select send arm completes -> REJECT A1b (HYB-E3003).
//
// ---- Differential Go 1.26 Oracle & Race Detector Verification ----
//   Without Hybrid's A1b rejection, concurrent execution of the receiver write
//   and sender write triggers FATAL: concurrent map writes under Go 1.26.
//   Verified by 	ools/check_corpus_census.py and 	ools/corpus_harness.py.
//
// ---- Adversarial Coverage Matrix (Stmt::Task vs Stmt::Send vs SelectArm) ----
//   1. Stmt::Task boundary: covered by struct-chan-scalars-accept.hy (#4208)
//      and struct-chan-shared-map-reject.hy (#4208).
//   2. Stmt::Send top-level channel send: covered by
//      struct-chan-scalars-send-accept.hy (#4209) and
//      struct-chan-shared-map-send-reject.hy (#4209).
//   3. SelectArmKind::Send select clause send: covered by this pair (#4228),
//      ensuring the checker enforces identical deep-sendability (A1c) and
//      post-move alias-freeze (A1b / O5.2) invariants inside select arms.
//
// ---- Compiler Pass Interaction ----
//   - hy-syntax: parses select { packets <- move p: {} } into
//     Stmt::Select with SelectArmKind::Send { chan, value: Expr::Move }.
//   - hy-check::sendability: traverses SelectArmKind::Send payload type
//     recursively across all struct fields (packet.label, packet.count,
//     packet.done / packet.shared).
//   - hy-check::freeze: records alias freeze on m when move p crosses
//     the SelectArmKind::Send boundary.
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
    select {
        packets <- move p: {} // move in select send arm freezes the aliased map storage
    }
    m["after"] = 2            // [S2] write through a live alias — A1b
    <-done
}
