// ============================================================================
// 37-wrapper-alias-own-move-control — Checker corpus #37 (added 2026-10-15):
// the ACCEPT-side control for 36.
//
// PURPOSE: 32 pins the REJECT for a wrapper alias used after an owned move.
// This program holds the SAME wrapper alias — `alias := same(same(agg))` —
// but reads it only BEFORE the move, and touches neither `agg` nor `alias`
// afterwards. The widened alias collector must not turn a legal program into
// a false positive: holding an alias is legal, only a use after the freeze
// violates O5.2. Pairs with 32 the way 23 pairs with 22.
//
// ---- Tier-0 rule set applied to each value ----
// Layer 1 (Go-side GC): `agg`/`alias` (same map value), channels, `m` in sink.
// Layer 2 (Rust-side owned): none.
// Layer 3 (boundary handles): none.
//
// ---- Sendability / check sites (task spawn / channel send) ----
//   [S1] `task sink(results, done)` — channels: sendable.
//   [S2] `fmt.println` on `alias` BEFORE the move — read while unfrozen: legal.
//   [S3] `results <- move agg` — owned move: sendable; freeze begins here and
//        covers `agg` and `alias`, neither of which is used afterwards.
//   [S4] `<-done` — receive: legal.
//
// ---- Expected verdict (registered 2026-10-15) ----
// EXPECTED-VERDICT: ACCEPT
//   ACCEPT — every check-site is sendable and nothing is used after the
//   freeze; the wrapper alias exists but is never read or written once
//   `agg` has been moved. FALSE-POSITIVE GUARD for 36.
// ============================================================================

fn same(m map[string]int) map[string]int {
    return m
}

fn sink(results <-chan map[string]int, done chan<- bool) {
    m := <-results
    fmt.println("count: {len(m)}")
    done <- true
}

fn main() {
    agg := map[string]int{}
    agg["a"] = 1

    results := make(chan map[string]int, 1)
    done := make(chan bool, 1)
    task sink(results, done)          // spawn site [S1]

    alias := same(same(agg))          // wrapper alias: holding it is legal
    fmt.println("before: {len(alias)}")   // read BEFORE the move: legal

    results <- move agg               // owned move; freeze begins

    <-done                            // neither agg nor alias is used again
}
