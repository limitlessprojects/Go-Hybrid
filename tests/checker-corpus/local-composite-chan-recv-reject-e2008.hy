// ============================================================================
// local-composite-chan-recv-reject-e2008 — Checker corpus (2026-10-31): a
// receive through a local bound from a composite (`append(chs, ch)`) must
// reject with HYB-E2008 when the channel is send-only.
//
// PURPOSE: the "known remaining sibling" deliberately left open by the
// #1347/#2002 fixture (recv_composite_channel_operands.rs NOTE): the binding
// `sub` carried no inferred `ty`, so a later `<-sub[0]` in a type position
// (where the E3008 direction walk does not run) had nothing to project and
// passed with 0 diagnostics — emitting Go that fails `go build` with
// `invalid operation: cannot receive from send-only channel`. process_local
// now binds the composite-projected type (composite_type_expr fallback), so
// the Index projection in recv_chan_type_expr resolves `chan<- int` and the
// direction check fires.
//
// EXPECTED-VERDICT: REJECT BUILDABLE
// ============================================================================
package main

fn bad(chs []chan<- int, ch chan<- int) {
    sub := append(chs, ch)
    var a [<-sub[0]]int
    _ = a
}
