// ============================================================================
// map-elem-struct-slice-spawn-reject — Checker corpus (bounty track: "grow the
// adversarial corpus").
//
// PURPOSE: a MAP-LITERAL ELEMENT store of a struct carrying a SLICE field,
//   followed by an owned move of the map. Corpus 45 pins deep sendability at
//   depth 3 as slice > struct > map; corpus 54/55 pin struct field STORES at a
//   spawn site; no program pins a reference-bearing struct value stored as a
//   MAP VALUE, where the map literal itself is what materializes the edge. The
//   map header copied into the literal must keep aliasing the sender's slice
//   backing array, so the move of the map freezes it.
//
// RULE CITATION: A1b (freeze-after-send) read with A4 (Go-heap reads are
//   copies, not loans) and O4.6 (deep sendability): `map[string]holder{"a":
//   holder{xs: xs}}` copies the slice header into the map element, so `xs` and
//   `m["a"].xs` name one backing array; `task worker(move m, done)` transfers
//   the map, and the later `xs[0] = 9` is a write through a live alias.
//
// ---- Tier-0 rule set applied ----
//   Layer 1 (Go-side GC): `xs` ([]int — the backing array), `m`
//     (map[string]holder — the moved map), `m["a"].xs` (the slice-header copy
//     inside the map value), `done` (channel).
//   Layer 2 (Rust-side owned): none.
//   Layer 3 (boundary handles): none.
//
// ---- Sendability / check sites ----
//   [S1] `map[string]holder{"a": holder{xs: xs}}` — the element store copies
//        the struct value, and the copy carries the slice HEADER: it aliases
//        `xs`'s backing array.
//   [S2] `task worker(move m, done)` — a legal owned move of the map; it does
//        not transfer the element's slice storage away from `xs`.
//   [S3] `xs[0] = 9` — a write through the live alias after the freeze.
//        VIOLATION (A1b).
//
// ---- Expected verdict (hand-derived from VERDICTS.md A1b + A4 and SPEC
//      O4.6/O5.2 BEFORE running the checker) ----
// EXPECTED-VERDICT: REJECT A1b
// EXPECTED-DIAGNOSTIC: HYB-E3003
//   REJECT — A1b. The map element holds a shallow copy of the struct, so its
//   slice field is a reference into the same array the sender still names. A
//   checker that walks the map literal only for scalar element types would
//   accept this and let the receiver and the sender write one array.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

struct holder {
    xs []int
}

fn worker(m map[string]holder, done chan<- bool) {
    fmt.println("first: {m["a"].xs[0]}")
    done <- true
}

fn main() {
    xs := []int{1, 2, 3}
    m := map[string]holder{"a": holder{xs: xs}}   // [S1] slice-header copy in the map value
    done := make(chan bool, 1)
    task worker(move m, done)                     // [S2] owned move; deep storage stays aliased
    xs[0] = 9                                     // [S3] write through the live alias — A1b
    <-done
}
