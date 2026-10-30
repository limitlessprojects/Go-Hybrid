// ============================================================================
// block-alias-spawn-write-reject -- issue #399
//
// PURPOSE: pin the block-scoped may_share bypass. `alias = shared` executed
// INSIDE an `if` block used to restore the pre-block `may_share` set wholesale
// on scope_pop, erasing the edge the block just created. The identical
// assignment at function level always rejected; the block was the only
// difference between reject and accept.
//
// ---- Tier-0 rule set applied to each value ----
// Layer 1 (Go-side GC): `shared` is a mutable map; `alias = shared` inside the
//   block makes `alias` a live alias (O5.2). `task worker(move alias)` moves
//   the storage; `shared["main"]++` afterwards is a WRITE through a frozen
//   alias. Emitted Go is `go worker(alias)` + `shared["main"]++` -- the issue
//   reported `fatal error: concurrent map writes` 3/3 under go1.26.9.
// Layer 2/3: none.
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: REJECT A1b
//   REJECT -- A1b/E3003 (write through a live alias of a moved value).
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

fn worker(m map[string]int) {
    m["task"]++
}

fn main() {
    shared := map[string]int{}
    var alias map[string]int

    if len(shared) == 0 {
        alias = shared
    }

    task worker(move alias)

    shared["main"]++
}
