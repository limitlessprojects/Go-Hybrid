// ============================================================================
// 61-closure-return-capture-at-spawn — Checker corpus #61 (issue: the
// interprocedural tail of #109)
//
// PURPOSE: negative control for the closure-return laundering gap. The #109
// fix links an INLINE closure literal (`f := fn() { ...m... }`) to the
// reference-bearing bindings it captures, so a later `move` of either freezes
// the other (O5.2). A closure MANUFACTURED BY A HELPER
// (`fn make_fn(m map[string]int) fn() { return fn() { m["k"] = 1 } }`) is a
// plain call at the define site: `TypeShape::Closure` is not may-share and a
// call has no root ident, so `record_may_share`'s carries gate returned
// early, `f := make_fn(m)` recorded NO edge, and `task worker(move m)` left
// `f()` unfrozen — the emitted Go died with `fatal error: concurrent map
// writes` (reproduced 5/5 under go1.26.5 on the unfixed build).
//
// ---- Tier-0 rule set applied to each value ----
// Layer 1 (Go-side GC): `m` is a mutable map. The callee captures the map in
//   the returned closure, so the caller's `f` is a live alias of `m`'s
//   storage; `task worker(move m)` must therefore freeze `f` (O5.2) and the
//   post-move call is the alias-read form of A1b.
// Layer 2/3: none.
//
// ---- Sendability / check sites ----
//   [S1] `task worker(move m, done)` — the transfer.
//   [S2] `f()` after the spawn — a use of a frozen alias (the closure body
//        writes `m` in the callee's frame; the call reaches that storage).
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: REJECT A1b
//   REJECT — A1b/E3003 (frozen alias read after the value was moved away).
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

fn make_fn(m map[string]int) fn() {
    return fn() {
        m["k"] = 1
    }
}

fn worker(m map[string]int, done chan<- bool) {
    m["task"] = 1
    done <- true
}

fn main() {
    m := map[string]int{"k": 0}
    f := make_fn(m)                 // the alias edge this case pins

    done := make(chan bool, 1)
    task worker(move m, done)       // [S1] spawn site

    f()                             // [S2] alias use after the move
    <-done
}
