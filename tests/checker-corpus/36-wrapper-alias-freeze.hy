// ============================================================================
// 36-wrapper-alias-freeze — Checker corpus #36 (added 2026-10-15 to pin the
// wrapper tail of issue #73 / the shapes reported in issue #160).
//
// PURPOSE: corpus 26 pins the ONE-LEVEL call return (`alias := same(agg)`) and
// 31 pins the generic form. Both are recorded as binding aliases because the
// call's argument is a bare identifier. This program pins the same alias
// reached through a WRAPPER: the argument of the outer call is itself a call
// (`alias := same(same(agg))`), so `Expr::root_ident()` — which stops at a
// call — returned None and neither `record_alias` nor `record_may_share`
// recorded an edge. After `results <- move agg` the write through `alias`
// passed with 0 diagnostics while `go run -race` reported:
//
//   WARNING: DATA RACE
//   Write at ... by goroutine 7:  main.sink()
//   Previous write at ... by main goroutine:  main.main()
//
// (sibling wrapper forms with the same root cause, all fixed by the same
// collector: `same(agg)[0:2]`, `wrap{items: same(agg)}`, `box{items: agg}.items`
// and `take(box{items: agg})`; they are pinned as hy-check unit tests.)
//
// UNCONTESTED: identical entailment to 26 — the checker has no interprocedural
// summary proving the return is fresh, and wrapping the argument in another
// call changes nothing about the Go semantics (the same map value reaches
// `alias`). Nested twin of 26/31; the accept-side control is corpus 33.
//
// ---- Tier-0 rule set applied to each value ----
// Layer 1 (Go-side GC): `agg` and `alias` (same map value, two bindings),
//   `results`/`done` (channels), receiver-side `m` (owned after the move).
// Layer 2 (Rust-side owned): none.
// Layer 3 (boundary handles): none.
//
// ---- Sendability / check sites (task spawn / channel send) ----
//   [S1] `task sink(results, done)` — channels: sendable (safe-by-type).
//   [S2] `results <- move agg` — OWNED MOVE: sendable (A1c route 2). The
//        A1b freeze starts HERE and covers the binding AND every live alias
//        — `alias` (aliased through the nested call return) is frozen too.
//   [S3] `done <- true` in sink — sendable.
//
// ---- Expected verdict (registered 2026-10-15) ----
// EXPECTED-VERDICT: REJECT A1b
//   REJECT — A1b alias-form: the write through `alias` is a use-after-move of
//   the value sent by `results <- move agg` (SPEC O5.2: the freeze covers the
//   binding and every live alias). Clean nested-call twin of 26.
// RUNTIME-HAZARD (pre-fix shape only): the pre-fix checker accepted this with
//   0 diagnostics; at runtime `sink` and `main` write the same map — the
//   harness/differential accept-side would see `fatal error: concurrent map
//   writes`. After the fix the program must NOT build.
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

    alias := same(same(agg))          // WRAPPER alias — outer arg is a call;
                                      // still the same map value as `agg`

    results <- move agg               // send site [S2] — LEGAL owned move;
                                      // freeze begins: agg AND alias frozen

    alias["total"] = len(alias)       // VIOLATION (A1b, alias-form): write
                                      // through a live alias of the moved value

    <-done
}
