// ============================================================================
// 57-callee-store-fresh-control — Checker corpus #57 (#455)
//
// PURPOSE: the over-rejection guard for corpus 56. A declared call that does
// NOT store one reference-bearing argument through another must not create a
// may-share edge, even when both arguments are live maps: the helper only
// reads them. This pins the effect scan to actual argument stores rather than
// "any call with two reference arguments".
//
// ---- Tier-0 rule set applied to each value ----
// Layer 1: `a` and `b` are distinct mutable maps; `pick` reads both and
//   returns a primitive. `move a` freezes only `a`; `b` stays writable.
// Layer 2/3: none.
//
// ---- Sendability / check sites ----
//   [S1] `task worker(move a)` — sendable via ownership transfer.
//   [S2] `b["main"]++` — a write to a value that shares nothing with `a`.
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: ACCEPT
//   ACCEPT — the call retains no argument through the other argument.
// ============================================================================

fn pick(a map[string]int, b map[string]int) int {
    if len(a) > len(b) {
        return len(a)
    }
    return len(b)
}

fn worker(m map[string]int) {
    for i in 1..=200000 {
        m["task"]++
    }
}

fn main() {
    a := map[string]int{}
    b := map[string]int{}
    _ = pick(a, b)
    task worker(move a)
    for i in 1..=200000 {
        b["main"]++
    }
}
