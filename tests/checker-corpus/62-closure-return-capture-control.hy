// ============================================================================
// 62-closure-return-capture-control — Checker corpus #62 (issue: the
// interprocedural tail of #109)
//
// PURPOSE: the over-rejection guard for corpus 61. The same helper shape with
// NO boundary crossing stays accepted (A5): a closure capturing mutable
// state on one task is outside the checked subset until a spawn/send/managed
// site appears. It also pins the precision half of the fix: a closure whose
// captured arguments are all PRIMITIVE (`make_adder(base int)`) creates no
// may-share root, so moving the primitive and calling the closure is a
// legitimate program (the receiver owns its copy).
//
// ---- Tier-0 rule set applied to each value ----
// Layer 1 (Go-side GC): the first `main` has a captured map but never moves
//   it; the second moves an INT (a copy — separate storage).
// Layer 2/3: none.
//
// ---- Sendability / check sites ----
//   none in the first main (A5, no boundary statement);
//   [S1] `task worker(move base, done)` — a primitive move in the second.
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: ACCEPT
//   ACCEPT — no check-site fires: the map never crosses a boundary, and the
//   primitive capture shares no storage with the moved copy.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

fn make_fn(m map[string]int) fn() {
    return fn() {
        m["k"] = 1
    }
}

fn make_adder(base int) fn() {
    return fn() {
        base = base + 1
    }
}

fn worker(n int, done chan<- bool) {
    done <- true
}

fn main() {
    m := map[string]int{"k": 0}
    f := make_fn(m)
    f()                             // one task, no boundary — A5

    base := 0
    g := make_adder(base)
    done := make(chan bool, 1)
    task worker(move base, done)    // [S1] primitive move: no shared storage
    g()
    <-done
    fmt.println("{len(m)}")
}
