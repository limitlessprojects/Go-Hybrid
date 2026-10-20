// ============================================================================
// slice-elem-map-alias-at-spawn — Checker corpus (bounty track: "grow the adversarial corpus").
//
// PURPOSE: the SLICE twin of map-elem-map-alias-at-spawn. Corpus 43 pins `copy` on a
//   `[]map` element slice, but no program reads a single reference-typed
//   SLICE ELEMENT into a binding. `inner := rows[0]` copies a map header, so
//   the copy aliases the map the slice element holds.
//
// RULE CITATION: A4 + A1b, as in the map-element form: the element read yields a copy of a
//   reference, and a copy of a reference is an alias.
//
// ---- Tier-0 rule set applied ----
//   Layer 1 (Go-side GC): `rows` ([]map[string]int), `inner` (the map read
//     out of `rows[0]`), `done` (channel).
//   Layer 2 (Rust-side owned): none.

// ---- Sendability / check sites ----
//   [S1] `task worker(move rows, done)` — legal owned move.
//   [S2] `inner["a"] = 2` — write through a live alias of the moved storage.

// ---- Expected verdict (hand-derived from VERDICTS.md rule key + SPEC
//      O3/O4/O9 BEFORE running the checker) ----
// EXPECTED-VERDICT: REJECT A1b
//   REJECT — A1b. The element read is a copy of a reference (A4), so the
//   binding aliases the moved slice's element storage.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

fn worker(rows []map[string]int, done chan<- bool) {
    fmt.println("a: {rows[0]["a"]}")
    done <- true
}

fn main() {
    rows := []map[string]int{map[string]int{"a": 1}}
    inner := rows[0]              // element read: a map-header copy — an alias
    done := make(chan bool, 1)
    task worker(move rows, done)
    inner["a"] = 2                // use-after-move through the alias
    <-done
}
