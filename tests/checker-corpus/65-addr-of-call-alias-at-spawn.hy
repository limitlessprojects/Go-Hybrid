// ============================================================================
// 65-addr-of-call-alias-at-spawn — Checker corpus #65 (issue #435)
//
// PURPOSE: `&h` laundered through a call. `collect_ref_roots` walked Try,
// Slice, Field, Index, Call and the literals but had no address-of arm, so the
// argument `&h` contributed no root at all: `p` was never linked to `h`, and
// `move h` froze nothing that `p` could still reach.
//
// ---- Tier-0 rule set applied to each value ----
// Layer 1 (Go-side GC): `&h` creates a borrow of `h`; `keep` returns it and
//   `p` keeps it live. `move h` transfers ownership, so every live alias of
//   `h` — including a pointer obtained through `&h` — must freeze (O5.2).
// Layer 2/3: none.
//
// ---- Sendability / check sites ----
//   [S1] `task worker(move h)` — the transfer.
//   [S2] `p.m["main"]++` after it — a WRITE through the laundered pointer.
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: REJECT A1b
//   REJECT — A1b/HYB-E3003. Accepting this program is a P0: the emitted Go
//   spawns `worker(h)` and keeps writing the same map through `p`
//   (`fatal error: concurrent map read and map write`, reproduced 5/5).
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

struct holder {
    m map[string]int
}

fn keep(h *holder) *holder {
    return h
}

fn worker(h holder) {
    h.m["task"]++
}

fn main() {
    var h holder
    h.m = map[string]int{}

    p := keep(&h)                   // the address-of laundered through a call

    task worker(move h)             // [S1] spawn site

    p.m["main"]++                   // [S2] write through the pointer
}
