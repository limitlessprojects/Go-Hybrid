// ============================================================================
// ptr-to-map-at-spawn — Checker corpus (bounty track: "grow the adversarial corpus").
//
// PURPOSE: a POINTER TO A MAP (`*map[string]int`, 0/60) at a spawn site, with nothing
//   else holding the map. Corpus 03 rejects `&stats` — a pointer to a shared
//   struct — but a pointer to a map is a distinct shape and it pins the same
//   question one level down: does the rule depend on the pointee being shared
//   elsewhere, or only on the type?
//
// RULE CITATION: A1c (safe-by-type) at the spawn site: `*map[string]int` is a mutable GC
//   pointer, not channel/atomic/immutable, not `move`d, not `managed` — not
//   sendable. The rule is TYPE-based, so it does not consult whether another
//   binding still names the map.
//
// ---- Tier-0 rule set applied ----
//   Layer 1 (Go-side GC): `m` (map[string]int), `p` (the *map pointing at it),
//     `done` (channel).
// ---- Sendability / check sites ----
//   [S1] `task worker(p, done)` — VIOLATION: a pointer to mutable GC state.
//        `m` is deliberately NOT written after the spawn, so the fixture
//        isolates the type rule from the liveness question.
// ---- Expected verdict (hand-derived from VERDICTS.md rule key + SPEC O4
//      BEFORE running the checker) ----
// EXPECTED-VERDICT: REJECT A1c
// EXPECTED-DIAGNOSTIC: HYB-E3001
//   REJECT — A1c. A pointer to a map grants the task the same mutable storage the
//   map itself would, so it cannot be more sendable than the map.
//   CONTESTED: the rule as written rejects this, and the checker agrees; if the
//   maintainer rules the shape should be legal, this row moves to ACCEPT by a
//   logged amendment (VERDICTS.md) — it is registered as REJECT because that is
//   what the rule key says today.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

fn worker(p *map[string]int, done chan<- bool) {
    done <- true
}

fn main() {
    m := map[string]int{"a": 1}
    p := &m
    done := make(chan bool, 1)
    task worker(p, done)   // [S1] VIOLATION: pointer to mutable GC state
    <-done
}
