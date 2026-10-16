// ============================================================================
// 50-move-closure-container-reject.hy - Checker corpus #50 (added WITH the
// fix; the program raced on main c09e992 before it landed).
//
// PURPOSE: moving a bare closure is rejected (HYB-E3001, A5 perimeter:
// moving a closure binding does not move or freeze what it captures). The
// SAME closure wrapped in a slice / map / struct literal escaped that rule:
// `move fs` froze only `fs`, never the captured `m`, so main kept writing
// `m` while the spawned task wrote it through `fs[0]()`. On main this was
// ACCEPT with 0 diagnostics, and `hyb build --emit-go` + `go run -race`
// reported 2 data races (exit 66). The fix walks the moved container's
// element / field shapes and rejects when any of them is a closure.
//
// ---- Expected verdict (registered with the fix, 2026-10-15) ----
// EXPECTED-VERDICT: REJECT A1c
//   REJECT - A1c (sendability): the moved value carries closures whose
//   captured environment stays reachable from the sender.
//
// ---- Annotations / managed ----
// lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

fn worker(fs []fn(), done chan<- bool) {
    for i in 0..1000 { fs[0]() }
    done <- true
}

fn main() {
    m := map[string]int{}
    fs := []fn(){fn() { m["k"]++ }}
    done := make(chan bool, 1)
    task worker(move fs, done) // VIOLATION: the closure inside still aliases m
    for i in 0..1000 { m["k"] = i }
    <-done
    fmt.println("{len(m)}")
}
