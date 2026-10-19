// ============================================================================
// atomic-pointer-fail-closed-reject — Additive checker corpus (corpus GROWTH,
// added AFTER the checker existed — the "adversarial corpus tests" track of
// BOUNTY.md, not a rewrite of a pre-registered verdict).
//
// PURPOSE: pin GUARANTEES.md row 11 ("sync/atomic (atomic.Pointer[T], …) —
// [TBD] joins the curated surface post-MVP; until then: unproven → rejected").
//
// Unlike the scalar sync/atomic types (atomic.bool, atomic.int64, etc. which
// are sendable-by-type class 2 and proven safe in corpus #17), atomic.pointer
// and atomic.value hold references to arbitrary mutable GC objects.
// While pointer swapping is internally synchronized, the pointee's memory
// is NOT synchronized. Crossing a task boundary without proving the pointee
// is safe would permit concurrent data races on the published pointer
// (Go runtime fatal: concurrent map writes).
//
// Tier-0 rule FC (SPEC-LANGUAGE §9.3 O4.9) requires fail-closed rejection:
// an unproven foreign atomic pointer MUST be rejected with HYB-E3005 at any
// task spawn or send check-site.
//
// ---- Tier-0 rule set applied to each value ----
// Layer 1 (Go-side GC): `p` (atomic.pointer[holder] — unproven pointee shape;
//   holding GC reference to `holder.data` map[string]int), `shared` (holder
//   struct holding mutable map), `done` (channel — safe-by-type).
// Layer 2 (Rust-side owned): none.
// Layer 3 (boundary handles): none.
//
// ---- Sendability / check sites (task spawn / channel send) ----
//   [S1] `task worker(&p, done)`:
//        `done` — channel: sendable (safe-by-type).
//        `&p` — pointer to `atomic.pointer[holder]`: foreign atomic type
//        holding arbitrary mutable heap pointee. Unproven under Tier-0
//        class 2 (which covers scalar atomics only).
//        Rule FC (O4.9): unproven type shape rejected fail-closed with
//        HYB-E3005, citing GUARANTEES.md row 11.
//   [S2] `done <- true` — bool literal: sendable.
//
// ---- Sibling shapes (same rule, documented so coverage is explicit) ----
//   S3 atomic.value: task worker_val(&v, done)  // REJECT FC (HYB-E3005)
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: REJECT FC
// EXPECTED-DIAGNOSTIC: HYB-E3005
//   REJECT — FC via the unproven atomic type path (HYB-E3005 at the spawn site).
//   Legal routes for the diagnostic: pass value across channel, wrap in
//   `managed("...", "atomic")`, or use a mutex.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
//   Checker-verdict test, not an execution test: no runtime output pinned.
// ============================================================================

import "sync/atomic"

struct holder {
    data map[string]int
}

fn worker(p *atomic.pointer[holder], done chan<- bool) {
    h := p.load()
    for i in 0..1000 {
        h.data["k"] = i
    }
    done <- true
}

fn main() {
    var p atomic.pointer[holder]
    shared := holder{data: map[string]int{"k": 0}}
    p.store(&shared)
    done := make(chan bool, 1)

    task worker(&p, done)

    <-done
}
