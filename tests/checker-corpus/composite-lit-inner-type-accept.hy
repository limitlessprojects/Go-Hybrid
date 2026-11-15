// ============================================================================
// composite-lit-inner-type-accept — Checker corpus (2026-11-03): the
// accept-side companion of composite-lit-inner-type-reject-e1002 — matching
// composite inners, chan direction assignability (bidirectional source into a
// directed element), pointers, arrays and map values all stay ACCEPT.
//
// PURPOSE: pins that deep_composite_conflict does not over-reject: identical
// spellings pass, a bidirectional channel feeds a receive-only element (spec
// §Assignability), and every non-composite side stays untouched.
//
// EXPECTED-VERDICT: ACCEPT
// ============================================================================
package main

struct box {
    val int
}

fn sink[t any](x t) {}

fn main() {
    var ok_chan chan int32
    let s = []chan int32{ ok_chan }
    sink(s)

    var bidir chan int32
    let r = []<-chan int32{ bidir }
    sink(r)

    var ok_arr [3]int
    let a = [1][3]int{ ok_arr }
    sink(a)

    var ok_ptr *box
    let p = []*box{ ok_ptr }
    sink(p)

    let m = map[string]chan int32{ "k": ok_chan }
    sink(m)
}
