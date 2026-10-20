// ============================================================================
// deep-composite-move-shared-at-spawn — Checker corpus (bounty track: "grow the adversarial corpus").
//
// PURPOSE: GUARANTEES.md claim 3 promises DEEP sendability: "every reference-typed
//   field/element must itself be sendable; a fresh literal wrapping a shared map is
//   still rejected". Corpus 45 pins that at depth 2 (slice > map) but rejects at the
//   SAFE-BY-TYPE gate — it sends without `move`, so the deep walk is never reached.
//   This pair uses `move` on BOTH halves, which removes the safe-by-type question
//   entirely and isolates the deep-embedding question at depth 3.
//
// RULE CITATION: A1b (freeze-after-send), read with O4.6 deep sendability: moving a composite
//   transfers the composite, but a reference-typed value embedded in it is still
//   aliased by the outer binding that named it, so the freeze must reach it.
//
// ---- Tier-0 rule set applied ----
//   Layer 1 (Go-side GC): `shared` (map[string]int — the embedded map),
//     `xs` ([]holder — moved), `holder.m` (the field holding `shared`),
//     `done` (channel).
//   Layer 2 (Rust-side owned): none.
// ---- Sendability / check sites ----
//   [S1] `task worker(move xs, done)` — a legal owned move: `xs` is not shared
//        with anyone, so A1c is satisfied and the safe-by-type gate does not fire.
//   [S2] `shared["a"] = 2` — a write through `shared`, which is still a live alias
//        of the map now living inside the moved `xs`: use-after-move (A1b).
//        The worker reads `xs[0].m` — the same hash table — so this is a
//        concurrent map read/write (a Go runtime fatal).
// ---- Expected verdict (hand-derived from VERDICTS.md rule key + SPEC O4/O5
//      BEFORE running the checker) ----
// EXPECTED-VERDICT: REJECT A1b
// EXPECTED-DIAGNOSTIC: HYB-E3003
//   REJECT — A1b. The move does not launder the embedded reference: a shallow
//   composite copy carries the map HEADER, so `shared` and `xs[0].m` name one hash
//   table. This is corpus 54's finding (a field store materializes the edge) taken
//   one level deeper — through a struct nested inside a slice — and isolated from
//   safe-by-type by the `move`.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

struct holder {
    m map[string]int
}

fn worker(xs []holder, done chan<- bool) {
    fmt.println("n: {len(xs)}")
    done <- true
}

fn main() {
    shared := map[string]int{"a": 1}
    xs := []holder{holder{m: shared}}   // depth 3: slice > struct > map
    done := make(chan bool, 1)
    task worker(move xs, done)          // [S1] legal move; deep embedding must still reject
    shared["a"] = 2                     // [S2] write through the live alias
    <-done
}
