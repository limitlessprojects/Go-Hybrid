// ============================================================================
// atomic-value-by-reference-accept — Additive checker corpus (corpus GROWTH,
// added AFTER the checker existed — the "adversarial corpus tests" track of
// BOUNTY.md, not a rewrite of a pre-registered verdict).
//
// PURPOSE: the ACCEPT twin of atomic-value-by-value-reject (issue #4457) — the
// mediated route of corpus #17 / OQ6. An atomic crosses soundly only as
// `&atomic_binding`: the receiver operates on the SAME instance, so its
// `add`/`store` are visible to the sender, and `go vet`'s copylocks stays
// silent (it fires on the by-value call). This fixture is what the E3001 fix-it
// asks for, and it must keep being accepted — the reject side must never grow
// to cover `&`.
//
// It pins both mediated cross sites:
//   [S1] spawn: `&hits` (and the auto-addressed `task add_auto(hits, done)`
//        against a `*atomic.int64` parameter, same instance, issue #694).
//   [S2] send: `ch <- &v` over a `chan *atomic.int64` — the only sound
//        channel route, since a channel cannot auto-address its element type.
//
// The send half is drained without using the binding: a received pointer does
// not have its foreign type attached at emit, so `p.add(1)` stays snake_case
// and does not build (issue #4458) — and the package form `atomic.add_int64`
// is the pre-1.19 API that wants a `*int64`, not a `*atomic.int64`. Both halves
// are still worth routing; only the use of the received binding is blocked.
//
// ---- Tier-0 rule set applied ----
// Layer 1 (Go-side GC): `hits`/`v` (atomic.int64) reached through `*atomic.int64`
//   pointers — the mediated share (OQ6); `done` (chan int), `ch`
//   (chan *atomic.int64) — safe-by-type; `samples` (chan int) — safe-by-type.
// Layer 2 (Rust-side owned): none.
// Layer 3 (boundary handles): none.
//
// ---- Sendability / check sites ----
//   [S1] `task bump(&hits, done)` — pointer to atomic: sendable (OQ6). PASS.
//   [S2] `task add_auto(auto, done)` — bare `auto` against a `*atomic.int64`
//        parameter: auto-addressed to `&auto`. PASS.
//   [S3] `ch <- &v` — pointer to atomic over a channel: sendable (OQ6). PASS.
//   [S4] `done <- 1`, `ch`/`samples` sends — safe-by-type. PASS.
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: ACCEPT
// EXPECTED-OUTPUT: 500 10 100
//   ACCEPT — every crossing is mediated by `&`; the same-instance guarantee
//   holds, so this is the shape row 11 should name.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

import "sync/atomic"

const calls = 500

fn bump(h *atomic.int64, done chan<- int) {
    for i in 0..calls {
        h.add(1)                   // SAME instance as main's `hits`
    }
    done <- 1
}

fn add_auto(h *atomic.int64, done chan<- int) {
    h.add(10)                      // `task add_auto(auto, done)` auto-addresses
    done <- 1
}

fn main() {
    var hits atomic.int64
    var auto atomic.int64
    var v atomic.int64
    done := make(chan int, 2)
    ch := make(chan *atomic.int64, 1)

    task bump(&hits, done)         // [S1] mediated spawn
    task add_auto(auto, done)      // [S2] auto-addressed spawn
    ch <- &v                       // [S3] mediated send
    _ = <-ch                       // drained unused: the received binding
                                   // loses the atomic type (#4458), so the
                                   // package form is no good either —
                                   // `atomic.add_int64` wants a `*int64`
                                   // (pre-1.19 API), not a `*atomic.int64`.
    v.add(100)                     // main writes the same instance

    _ = <-done
    _ = <-done          // one per spawn
    fmt.println("{hits.load()} {auto.load()} {v.load()}")   // 500 10 100
}
