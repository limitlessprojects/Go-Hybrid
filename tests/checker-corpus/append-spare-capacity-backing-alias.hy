// ============================================================================
// append-spare-capacity-backing-alias — Checker corpus (bounty track: "grow the adversarial corpus").
//
// PURPOSE: `append` on a slice that still has SPARE CAPACITY returns a slice sharing
//   the same backing array. Corpus 53 rejects `append(a[0:1], x)` on an ARRAY
//   in its rationale; no program pins the `make([]T, len, cap)` form, where
//   the spare capacity is exactly what makes the append alias. Corpus 51
//   pins the 3-argument `make` form itself as an ordinary fresh local — this
//   program pins that a slice DERIVED from it by append is not independent.
//
// RULE CITATION: A4 + A1b: `append` may return the same backing array, so the result is a
//   live alias of the source slice's storage.
//
// ---- Tier-0 rule set applied ----
//   Layer 1 (Go-side GC): `shared` ([]int, len 1 cap 4 — moved), `alias` (the
//     slice `append` returned over the same array), `done` (channel).
//   Layer 2 (Rust-side owned): none.

// ---- Sendability / check sites ----
//   [S1] `task worker(move shared, done)` — legal owned move.
//   [S2] `alias[0] = 9` — element write into the array the move transferred
//        (the append fit in the spare capacity, so no reallocation happened).

// ---- Expected verdict (hand-derived from VERDICTS.md rule key + SPEC
//      O3/O4/O9 BEFORE running the checker) ----
// EXPECTED-VERDICT: REJECT A1b
//   REJECT — A1b. The append result aliases the source backing array whenever
//   the capacity allows; the checker must not treat it as fresh.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

fn worker(xs []int, done chan<- bool) {
    fmt.println("first: {xs[0]}")
    done <- true
}

fn main() {
    shared := make([]int, 1, 4)   // len 1, cap 4 — append fits without reallocating
    alias := append(shared, 1)    // same backing array: a live alias
    done := make(chan bool, 1)
    task worker(move shared, done)
    alias[0] = 9                  // element write into the moved array
    <-done
}
