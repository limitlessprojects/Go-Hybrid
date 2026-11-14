// ============================================================================
// atomic-value-by-value-reject — Additive checker corpus (corpus GROWTH, added
// AFTER the checker existed — the "adversarial corpus tests" track of
// BOUNTY.md, not a rewrite of a pre-registered verdict).
//
// PURPOSE: pin that class 2 for `sync/atomic` scalars is only true BEHIND `&`
// (issue #4457). atomic-typed values are sendable-by-type
// (`is_sendable_by_type`, hy-check/src/model.rs — `TypeShape::Atomic`), and
// the class-2 rationale given in SPEC-LANGUAGE O4 is "internally synchronized
// by construction". That rationale holds for ONE instance: a value crossed BY
// VALUE is a copy, so the receiver gets its own atomic and every `add`/`store`
// it performs is lost at return. The synchronization the class credits never
// spans the boundary — it is `int`-by-value with a lock bolted on, and
// `go vet`'s copylocks agrees (fires on the by-value call, silent on `&`).
//
// Before the issue-#4457 fix this program was ACCEPTED with 0 diagnostics and
// printed `hits=0`; the `&hits` twin (atomic-value-by-reference-accept)
// printed `hits=1`. Nothing in the accepted program told the two apart.
//
// ---- Tier-0 rule set applied ----
// Layer 1 (Go-side GC): `hits` (atomic.int64 — class 2 by type), `done`
//   (chan int — safe-by-type).
// Layer 2 (Rust-side owned): none.
// Layer 3 (boundary handles): none.
//
// ---- Sendability / check sites ----
//   [S1] `task bump(hits, done)`:
//        `hits` — bare atomic value at a spawn against a VALUE parameter:
//        a copy crosses. VIOLATION (A1c): class 2 does not cover a copy of
//        the instance. The fix is `&hits` against `*atomic.int64` (OQ6).
//        `done` — channel: sendable.
//   [S2] `done <- 1` — int literal: sendable.
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: REJECT A1c
// EXPECTED-DIAGNOSTIC: HYB-E3001
//   REJECT — A1c at the spawn site. The fix-it names the mediated route
//   (`&hits`); the accept twin is atomic-value-by-reference-accept.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

import "sync/atomic"

fn bump(a atomic.int64, done chan<- int) {
    a.add(1)                       // writes land in the checkout copy
    done <- 1
}

fn main() {
    var hits atomic.int64
    done := make(chan int, 1)

    task bump(hits, done)          // SPAWN SITE — VIOLATION (A1c): by VALUE

    _ = <-done
    fmt.println("hits={hits.load()}")   // 0, not 1
}
