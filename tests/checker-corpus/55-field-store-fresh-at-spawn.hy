// ============================================================================
// 55-field-store-fresh-at-spawn — Checker corpus #55 (issue #392)
//
// PURPOSE: the positive control for #53. It pins that the #392 fix (a field
// store materializes a may-share edge from the composite's root binding to
// live reference-bearing roots on the RHS) does NOT over-reject: storing a
// FRESH map literal into a field and then moving the composite into a task
// shares no live storage, so the program must stay ACCEPT with zero `managed`.
//
// A checker that rejects this is over-strict — that would be a bug in the
// checker, not in the corpus.
//
// ---- Tier-0 rule set applied to each value ----
// Layer 1 (Go-side GC): `map[string]int{}` is a fresh literal with no live
//   alias; `var h holder` owns it after the store; `move h` transfers the
//   whole composite. No value is reachable from two tasks.
// Layer 2/3: none.
//
// ---- Sendability / check sites ----
//   [S1] `task worker(move h)` — owned move of a composite whose only
//        reference-typed field was built in place.
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: ACCEPT
//   ACCEPT — 0 annotations, 0 managed.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

struct holder {
    m map[string]int
}

fn worker(h holder) {
    h.m["task"]++
}

fn main() {
    var h holder
    h.m = map[string]int{}          // FRESH literal: no live alias to share

    task worker(move h)             // [S1] owned move of a self-contained value

    fmt.println("worker spawned")
}
