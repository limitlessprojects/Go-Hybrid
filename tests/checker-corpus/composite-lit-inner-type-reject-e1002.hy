// ============================================================================
// composite-lit-inner-type-reject-e1002 — Checker corpus (2026-11-03): a
// composite literal element/key/value whose composite INNER type differs must
// reject with HYB-E1002.
//
// PURPOSE: the kind model collapses container inners — `OpKind::Chan(Int)`
// fits `chan int64` as well as `chan int32` — so `[]chan int32{ bad_chan }`
// with `bad_chan chan int64`, `[1][3]int{ bad_arr }` with `bad_arr [4]int`,
// and `map[string]chan int32{ "k": bad_chan }` all passed with 0 diagnostics
// and failed `go build` (`cannot use ... as ... value in slice/array/map
// literal`). buildable.rs now deep-compares the resolved spellings
// (deep_composite_conflict): same container family, a textual part mismatch
// conflicts. Channel direction mismatches stay with the E3012 coercion
// layer (a second diagnostic on the same span is a double report); aliases
// and type parameters fail open.
//
// EXPECTED-VERDICT: REJECT BUILDABLE
// ============================================================================
package main

fn sink[t any](x t) {}

fn main() {
    var bad_chan chan int64
    let s = []chan int32{ bad_chan }
    sink(s)

    var bad_arr [4]int
    let a = [1][3]int{ bad_arr }
    sink(a)

    let m = map[string]chan int32{ "k": bad_chan }
    sink(m)
}
