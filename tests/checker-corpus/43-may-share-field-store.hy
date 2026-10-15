// ============================================================================
// 43-may-share-field-store — Checker corpus #43 (soundness issue #92,
// reported against main 8fb9238; field/index store may-share recording).
//
// PURPOSE: pin O5.2 / A1b on a store into a field/index of a live base.
// Tier-0 `record_may_share` only fired for a bare `Ident` rebind target, so
// `h.m = m` recorded no edge between the base `h` and the value `m` — while
// the composite-literal twin `h := holder{m: m}` (already rejected, corpus
// family 26+) recorded the edge. A later `task worker(move m, done)` left
// `h` unfrozen and main kept writing through the moved map header: before
// the fix `hyb check` reported OK (0 diagnostics) while `go run` crashed
// with `fatal error: concurrent map writes`. The fix records may-share from
// the place's root binding to the value's ref roots on every field/index
// store, closing the asymmetry.
//
// ---- Tier-0 rule set applied to each value ----
// Layer 1 (Go-side GC): `m` (map[string]int - env - GC), `h` (holder
//   holding m), `done` (chan bool - env - GC).
// Layer 2: `move m` is the sole ownership transfer; the worker owns `m`.
// Layer 3 (A1b): `h.m = m` hands the map header to `h`'s field, so `h` is a
//   live alias of `m`'s storage; `h.m["k"] = i` after the move is a WRITE
//   through that alias (O5.2).
// Layer 4 (A3): no map-slot loan is open; this row is pure O5.2.
// Layer 5 (A5): the worker body mutates `m` (its own); the main loop
//   mutates the same map through `h.m`.
// Layer 6 (O5.1): all storage is main-owned; nothing crosses as non-owned.
// Layer 7 (A7): `done <- true` hands the bool; buffered cap 1.
// Layer 8 (FC): the verdict is a rule rejection, not an unproven-name FC.
//
// EXPECTED: HYB-E3003 (write through a live alias of a moved value,
//   tier0_rule A1b / O5.2) at `h.m["k"] = i`.
//
// ============================================================================
// EXPECTED-VERDICT: REJECT A1b

struct holder {
    m map[string]int
}

fn worker(m map[string]int, done chan<- bool) {
    for i in 0..10000 {
        m["k"] = i
    }
    done <- true
}

fn main() {
    m := map[string]int{"k": 0}
    h := holder{m: map[string]int{"init": 0}}
    h.m = m
    done := make(chan bool, 1)
    task worker(move m, done)
    for i in 0..10000 {
        h.m["k"] = i
    }
    <-done
}