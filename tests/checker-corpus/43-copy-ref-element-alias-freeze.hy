// ============================================================================
// 43-copy-ref-element-alias-freeze — Checker corpus #43 (added 2026-10-15 to
// pin issue #178: the builtin `copy(dst, src)` is not modelled, so for a
// slice whose ELEMENT carries mutable GC storage the destination and source
// become may-share peers with no edge, and a later `move` does not freeze
// the source).
//
// PURPOSE: `copy` is element-wise; when the element type is a reference
// (map/slice/pointer, or a struct/array of them) it duplicates the element
// *references*, so `dst[0]` and `src[0]` alias the exact same map. After
// `task worker(move dst, done)` the checker must freeze `src` too (O5.2
// "every live alias"). Before the fix `hyb check` reported 0 diagnostics
// while `go run -race` reported a DATA RACE and `fatal error: concurrent
// map writes`.
//
// UNCONTESTED: element-reference sharing is entailed by any sound freeze
// rule (SPEC O5.2); `copy` is just another way to spell the alias. Clean
// twin of 26 (call-return alias) and 29 (shallow struct copy of a param);
// primitive-element accept-side control: 44.
//
// ---- Tier-0 rule set applied to each value ----
// Layer 1 (Go-side GC): `src`, `dst` (same map element after copy),
//   `done` (channel), receiver-side `d` (owned after the move).
// Layer 2 (Rust-side owned): none.
// Layer 3 (boundary handles): none.
//
// ---- Sendability / check sites (task spawn / channel send) ----
//   [S1] `task worker(move dst, done)` — `dst` is an owned move (A1c route 2);
//        the A1b freeze begins HERE and must cover `src` (a may-share peer
//        created by `copy`).
//   [S2] `done <- true` in worker — sendable.
//
// ---- Expected verdict (registered 2026-10-15) ----
// EXPECTED-VERDICT: REJECT A1b
// EXPECTED-DIAGNOSTIC: HYB-E3003
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

fn worker(d []map[string]int, done chan<- bool) {
    d[0]["k"] = 1
    done <- true
}

fn main() {
    src := []map[string]int{map[string]int{"k": 0}}
    dst := make([]map[string]int, 1)
    copy(dst, src)                 // dst[0] and src[0] are the SAME map

    done := make(chan bool, 1)
    task worker(move dst, done)    // freeze begins — must cover `src` via copy

    src[0]["k"] = 2                // VIOLATION (A1b): write through the peer

    <-done
}
