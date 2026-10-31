// ============================================================================
// local-composite-chan-recv-accept — Checker corpus (2026-10-31): the
// accept-side companion of local-composite-chan-recv-reject-e2008 — a local
// bound from a composite of BIDIRECTIONAL channels still receives fine,
// end to end: checker accept, emitted Go builds, program runs.
//
// PURPOSE: pins that the composite-type projection fallback in process_local
// does not over-reject: a correctly-directed `sub := append(chs, ch)` then
// `<-sub[0]` must stay ACCEPT end to end.
//
// IN-MERGE RESHAPE (2026-10-31, disclosed, fixup credited to the PR author):
// the original fixture received in a TYPE position — `var a [<-sub[0]]int` —
// which is not buildable Go ("array length ... must be constant"); the
// differential accept side (build+run) can never pass such a shape even with
// a perfect checker, so "ACCEPT end to end" was unpinnable as written. The
// receive moved to a value position the emitter supports and the projection
// fallback still serves (hyb check: 0 diagnostics; hyb build: go1.26.9 ok;
// run: exit 0 — verified at merge time). The type-position reject path stays
// pinned by local-composite-chan-recv-reject-e2008 +
// recv_local_composite_binding.rs.
//
// EXPECTED-VERDICT: ACCEPT
// ============================================================================
package main

fn main() {
    ch := make(chan int, 1)
    ch <- 7
    chs := make([]chan int, 0)
    chs = append(chs, ch)
    sub := append(chs, ch)
    v := <-sub[0]
    _ = v
}
