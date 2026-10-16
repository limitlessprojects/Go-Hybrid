// ============================================================================
// 44-copy-primitive-element-control — Checker corpus #44 (added 2026-10-15):
// the ACCEPT-side control for 43.
//
// PURPOSE: 43 pins the REJECT for `copy(dst, src)` on a reference-element
// slice. This program holds the SAME `copy` shape on a PRIMITIVE-element
// slice — `copy(dst, src)` on `[]int` — where the copy duplicates values,
// not references, so `dst` and `src` share NO mutable GC storage. Recording
// a may-share edge here would turn a legal program into a false positive:
// a `move` of `dst` must NOT freeze `src`. Pairs with 43 the way 41 pairs
// with 40 and 23 pairs with 22.
//
// ---- Tier-0 rule set applied to each value ----
// Layer 1 (Go-side GC): `src`/`dst` (`[]int`, independent backing arrays
//   after copy), `done` (channel), receiver-side `d`.
// Layer 2 (Rust-side owned): none.
// Layer 3 (boundary handles): none.
//
// ---- Sendability / check sites (task spawn / channel send) ----
//   [S1] `copy(dst, src)` — value copy of `[]int`; no sharing.
//   [S2] `task worker(move dst, done)` — owned move: sendable; freezes `dst`
//        only, never `src` (no shared storage).
//   [S3] `src[0] = 5` — write to the still-live, unshared local: legal.
//   [S4] `done <- true` / `<-done` — sendable / receive.
//
// ---- Expected verdict (registered 2026-10-15) ----
// EXPECTED-VERDICT: ACCEPT
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

fn worker(d []int, done chan<- bool) {
    d[0] = 9
    done <- true
}

fn main() {
    src := []int{1}
    dst := make([]int, 1)
    copy(dst, src)                 // ints copy by value — no shared storage

    done := make(chan bool, 1)
    task worker(move dst, done)    // legal: dst and src are independent

    src[0] = 5                     // legal: distinct backing arrays

    <-done
}
