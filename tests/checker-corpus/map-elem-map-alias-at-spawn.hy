// ============================================================================
// map-elem-map-alias-at-spawn — Checker corpus (bounty track: "grow the adversarial corpus").
//
// PURPOSE: a MAP WITH A REFERENCE-TYPED ELEMENT is an untested intersection: the
//   corpus exercises map-slot LOANS (08/09/22/23) and `[]map` only through
//   `copy` (43), but no program reads a map ELEMENT whose value is itself a
//   map. `inner := m["frontend"]` is a Go-heap read (A4) and therefore a
//   COPY — but the copied value is a map HEADER, so the copy aliases the same
//   hash table as the element it was read from. A4's copy rule must not be
//   read as "fresh storage" for reference-typed elements.
//
// RULE CITATION: A4, "Go-heap reads are copies, not loans" (SPEC-LANGUAGE.md O9), read
//   together with A1b, "freeze-after-send": the copy is a live ALIAS of the
//   moved value's storage, so `move m` freezes it too.
//
// ---- Tier-0 rule set applied ----
//   Layer 1 (Go-side GC): `m` (map[string]map[string]int), `inner` (the
//     map[string]int read out of `m` — a reference copy), `done` (channel).
//   Layer 2 (Rust-side owned): none.

// ---- Sendability / check sites ----
//   [S1] `task worker(move m, done)` — the move itself is legal (A1c route 2,
//        an owned move). `done` is a channel: safe-by-type.
//   [S2] `inner["replicas"] = 3` — a WRITE through a live alias of the moved
//        value, after the move: use-after-move (A1b). The worker reads
//        `services["frontend"]["replicas"]` while main writes the same inner
//        hash table: a concurrent map read/write, which is a Go runtime fatal.

// ---- Expected verdict (hand-derived from VERDICTS.md rule key + SPEC
//      O3/O4/O9 BEFORE running the checker) ----
// EXPECTED-VERDICT: REJECT A1b
//   REJECT — A1b. `inner` aliases the storage `move m` transferred, so the
//   write after the move is use-after-move. This is the reference-element
//   form of the freeze-defeat shape (20/26): the alias arrives through an
//   ELEMENT READ rather than a binding alias or a call return.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

struct env {
    services map[string]map[string]int
}

fn worker(services map[string]map[string]int, done chan<- bool) {
    fmt.println("frontend replicas: {services["frontend"]["replicas"]}")
    done <- true
}

fn main() {
    e := env{services: map[string]map[string]int{"frontend": map[string]int{"replicas": 2}}}
    m := e.services
    inner := m["frontend"]      // element read: a COPY of a map HEADER — aliases
                                // the same hash table (A4 read, not fresh storage)
    done := make(chan bool, 1)
    task worker(move m, done)   // [S1] legal owned move; freezes `m` and its aliases
    inner["replicas"] = 3       // [S2] write through the live alias after the move
    <-done
}
