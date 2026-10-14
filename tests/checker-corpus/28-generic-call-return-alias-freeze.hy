// ============================================================================
// 28-generic-call-return-alias-freeze — Checker corpus #28 (added 2026-10-14
// to pin issue #73: a generic call's result shape degrades to Unknown
// through the type parameter, so the alias recorder's reference-shape guard
// never fires and the alias escapes the freeze).
//
// PURPOSE: corpus 26 covers the CONCRETE map identity (`alias := same(agg)`,
// result shape `map[string]int` → recorded as a binding alias). This program
// covers the GENERIC identity: `same` is `fn same[t any](m t) t`, the result
// type is the type PARAMETER `t`, and the checker resolves it to Unknown —
// not a reference alias — so `record_alias`'s Call arm was skipped and NO
// alias/share edge was recorded for `alias`. After `results <- move agg` the
// write through `alias` passed with 0 diagnostics while `go run` reported
// `fatal error: concurrent map writes` (same emission shape as `go -race`:
// `go worker(agg)` writes `agg` while the sender writes `alias` — the SAME
// map store, two goroutines).
//
// The sibling shape from issue #73 — `h2 := alias_holder(h1)` where the fn
// returns its struct argument (result `Struct`, neither a binding alias nor
// previously edge-tracked) — is pinned by the hy-check unit tests; both
// share ONE root cause: a call result that hands back argument storage was
// invisible to `record_may_share` (no Call arm) and, for non-reference
// result shapes, invisible to `record_alias` too.
//
// UNCONTESTED: same entailment as 26 — a call is just another way to spell
// the alias; the generic wrapper changes nothing about the Go semantics
// (`t = map[string]int` after instantiation, `same` returns the same map).
// Clean generic twin of 26; lexical twin: 20.
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
//        — `alias` (aliased via the generic call return) is frozen too.
//   [S3] `done <- true` in sink — sendable.
//
// ---- Expected verdict (registered 2026-10-14) ----
// EXPECTED-VERDICT: REJECT A1b
//   REJECT — A1b alias-form (freeze covers every live alias, SPEC O5.2;
//   diagnostic HYB-E3003). Not contested: this is a write through an alias
//   of the moved value; the type parameter in `same`'s signature does not
//   weaken the freeze.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

fn same[t any](m t) t {
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

    alias := same(agg)                // ALIAS via GENERIC call return — same
                                      // map value; result shape is Unknown
                                      // (type param `t`), which used to
                                      // bypass both recorders

    results <- move agg               // send site [S2] — LEGAL owned move;
                                      // freeze begins: agg AND alias frozen

    alias["total"] = len(alias)       // VIOLATION (A1b, alias-form): write
                                      // through a live alias of the moved value

    <-done
}
