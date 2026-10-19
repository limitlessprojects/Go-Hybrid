// ============================================================================
// any-erasure-primitive-control — Checker corpus (bounty track: "grow the adversarial corpus").
//
// PURPOSE: the ACCEPT control for any-erasure-shared-map-at-spawn, and the more
//   important half of the pair: it proves the rejection above is not a blanket
//   ban on `any` at a boundary. An `any` whose value is an immutable primitive
//   carries no shared mutable storage, so it crosses.
//
// RULE CITATION: A1c: the value behind the erasure is a primitive, which is safe-by-type. The
//   `any` keyword is not itself the deciding factor.
//
// ---- Tier-0 rule set applied ----
//   Layer 1 (Go-side GC): `v` (any holding an int — immutable), `done`.
// ---- Sendability / check sites ----
//   [S1] `task worker(v, done)` — the value behind the erasure is immutable,
//        so there is nothing to share.
// ---- Expected verdict (hand-derived from VERDICTS.md rule key + SPEC O4
//      BEFORE running the checker) ----
// EXPECTED-VERDICT: ACCEPT
//   ACCEPT — 0 annotations, 0 managed. Pins that the checker classifies the value
//   behind the erasure, not the erasing type.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

fn worker(v any, done chan<- bool) {
    done <- true
}

fn main() {
    var v any = 42
    done := make(chan bool, 1)
    task worker(v, done)   // immutable behind the erasure: nothing is shared
    <-done
}
