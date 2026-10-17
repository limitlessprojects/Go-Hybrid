// ============================================================================
// 56-callee-store-alias-at-spawn — Checker corpus #56 (#455)
//
// PURPOSE: negative control for the helper-frame store gap. A store written in
// the analysed function itself (`dst[0] = v`, corpus 54's shape) now
// materializes the may-share edge and the later `move` freezes it. The same
// store performed inside a DECLARED HELPER (`fn set(dst, v) { dst[0] = v }`)
// was analysed in the callee's own FnState only: the caller's graph never saw
// the edge, `move` did not freeze `dst`, and both tasks wrote the same map.
//
// ---- Tier-0 rule set applied to each value ----
// Layer 1 (Go-side GC): `shared` is a mutable map written by two tasks. After
//   `set(dst, shared)`, `dst[0]` is an alias of `shared`, so `move shared` at
//   the spawn must freeze `dst` (O5.2).
// Layer 2/3: none.
//
// ---- Sendability / check sites ----
//   [S1] `task worker(move shared)` — `shared` is moved, but the helper-made
//        alias `dst[0]` is still live in the parent.
//   [S2] `dst[0]["main"]++` after the spawn — a WRITE through a frozen alias.
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: REJECT A1b
//   REJECT — A1b/E3003 (write through a live alias of a moved value).
//   Accepting this program is a P0: the emitted Go spawns `worker(shared)`
//   while `main` keeps writing the same map through `dst[0]`.
// ============================================================================

fn set(dst []map[string]int, v map[string]int) {
    dst[0] = v
}

fn worker(m map[string]int) {
    for i in 1..=200000 {
        m["task"]++
    }
}

fn main() {
    shared := map[string]int{}
    dst := []map[string]int{map[string]int{}}
    set(dst, shared)
    task worker(move shared)
    for i in 1..=200000 {
        dst[0]["main"]++
    }
}
