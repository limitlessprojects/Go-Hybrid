// ============================================================================
// struct-field-slice-elem-alias — Checker corpus (bounty track: "grow the adversarial corpus").
//
// PURPOSE: a struct FIELD whose type is a slice, read into a binding and then written
//   after the struct is moved. Corpus 54/55 cover field STORES and 53 covers
//   array/slice backing, but no program reads a reference-typed FIELD into a
//   local and then mutates the element storage through it after a move.
//
// RULE CITATION: A4 + A1b: `d := h.data` copies a slice header (a reference), so `d`
//   aliases the array `move h` transferred.
//
// ---- Tier-0 rule set applied ----
//   Layer 1 (Go-side GC): `h` (holder — moved), `d` (the slice header read out
//     of `h.data`), `done` (channel).
//   Layer 2 (Rust-side owned): none.

// ---- Sendability / check sites ----
//   [S1] `task worker(move h, done)` — legal whole-binding owned move.
//   [S2] `d[0] = 9` — element write through the aliased backing array.

// ---- Expected verdict (hand-derived from VERDICTS.md rule key + SPEC
//      O3/O4/O9 BEFORE running the checker) ----
// EXPECTED-VERDICT: REJECT A1b
//   REJECT — A1b. Field reads of reference type are copies of references; the
//   write reaches the storage the move transferred.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

struct holder {
    data []int
}

fn worker(h holder, done chan<- bool) {
    fmt.println("first: {h.data[0]}")
    done <- true
}

fn main() {
    h := holder{data: []int{1, 2, 3}}
    d := h.data                  // field read: a slice-header copy — an alias
    done := make(chan bool, 1)
    task worker(move h, done)
    d[0] = 9                     // element write through the aliased array
    <-done
}
