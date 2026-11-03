// ============================================================================
// option-map-alias-at-spawn — additive registration 2026-11-01 (bounty track).
//
// PURPOSE: `option[T]` is a value wrapper; T21 says it is sendable iff `T` is,
// and the owned-move freeze (O5.2 / A1b) is defined over the underlying storage,
// not over the wrapper's own fields. Wrapping a shared map in `some(...)` must
// not launder the alias: after `move o`, the payload's hash table is frozen, so
// the sender's write through `shared` is use-after-move.
//
// ---- Sendability / check sites ----
//   [S1] `task worker(move o, done)` — owned move of the option value. The
//        move freezes `o` and every live alias, including the captured map
//        header `shared` that `some(shared)` embedded.
//   [S2] `shared["main"]++` — write through the live alias after the move.
//
// ---- Expected verdict (derived from T21 + O5.2 before running) ----
// EXPECTED-VERDICT: REJECT A1b
// ============================================================================

fn worker(o option[map[string]int], done chan<- bool) {
    match o {
        some(m): m["task"]++
        none:    fmt.println("none")
    }
    done <- true
}

fn main() {
    shared := map[string]int{}
    o := some(shared)
    done := make(chan bool, 1)
    task worker(move o, done)
    shared["main"]++
    <-done
}
