// ============================================================================
// struct-chan-scalars-select-send-accept — Checker corpus (bounty track:
// "grow the adversarial corpus").
//
// PURPOSE: the SELECT-SEND companion of struct-chan-scalars-accept (#4208) and
//   struct-chan-scalars-send-accept (#4209). A struct carrying a CHANNEL plus
//   immutable scalar fields is moved through a `select` send clause
//   (`packets <- move p: {}`). Deep sendability (SPEC C5 + O4/A1c) must accept
//   it on the `SelectArmKind::Send` path: the channel field is safe-by-type,
//   the string/int fields are immutable/copied, and `move` transfers ownership
//   of the struct. This guards the `select`-send arm path against a checker
//   that only teaches `Stmt::Task` or top-level `Stmt::Send` about move-sent
//   struct values.
//
// RULE CITATION: SPEC C5 (select send clauses are boundary check-sites) + A1c
//   route 1 (safe-by-type channel + immutable scalars) with an owned `move`
//   at the select send arm.
//
// ---- Tier-0 rule set applied ----
//   Layer 1 (Go-side GC): `p` (packet value, moved), `p.done` (channel),
//     `p.label` (string — immutable), `p.count` (int — copied), `done` /
//     `packets` (channels).
//   Layer 2 (Rust-side owned): none.
//   Layer 3 (boundary handles): none.
//
// ---- Sendability / check sites ----
//   [S1] `packets <- move p: {}` — p's fields are all sendable and the binding
//        is moved in the select send arm; the sender never uses p again. ACCEPT.
//   [S2] `p.done <- true` — channel send inside the consumer; legal.
//
// ---- Expected verdict (hand-derived from VERDICTS.md A1c + C5 BEFORE running
//      the checker) ----
// EXPECTED-VERDICT: ACCEPT
//   ACCEPT — 0 annotations, 0 managed. A checker that rejects this is
//   over-strict on move-sent struct values inside a `select` send clause.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
//
// ---- Detailed Step-by-Step Ownership & Select-Arm Trace (SPEC C5 + A1c) ----
//   Step 1: done := make(chan bool, 1) allocates a safe-by-type channel
//           capability (Layer 1 GC-managed channel header, sendable by value).
//   Step 2: packets := make(chan packet, 1) allocates the typed transport
//           channel carrying packet struct values across goroutine tasks.
//   Step 3: p := packet{label, count, done} constructs an owned local
//           struct value whose fields are:
//             - label string: immutable Go string header (A1c safe-by-type)
//             - count int: value-copied scalar integer (A1c safe-by-type)
//             - done chan bool: safe-by-type synchronization channel
//   Step 4: 	ask consumer(packets) spawns the receiver goroutine holding a
//           receive-only view (<-chan packet) of the packets channel.
//   Step 5: select { packets <- move p: {} } evaluates the SelectArmKind::Send
//           boundary check-site (SPEC C5):
//             (a) move p consumes the local p binding in the sender scope;
//             (b) deep field traversal verifies every transitive field of packet
//                 satisfies A1c route 1 without requiring a managed block;
//             (c) no post-send read or write of p occurs after the select.
//   Step 6: <-done waits for the consumer task to acknowledge receipt.
//
// ---- Differential Go 1.26 Oracle & Race Detector Verification ----
//   Emitted Go program compiles cleanly under go vet ./... and executes
//   deterministically under go test -race with zero data-race reports.
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
    select {
        packets <- move p: {} // S1 — owned move in select send arm; all fields sendable
    }
    <-done
}
