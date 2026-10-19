// ============================================================================
// deep-composite-move-fresh-control — Checker corpus (bounty track: "grow the adversarial corpus").
//
// PURPOSE: the ACCEPT control for deep-composite-move-shared-at-spawn. Structurally
//   identical — same slice, same struct, same `move`, same channel — with ONE change:
//   the innermost map is a fresh literal, so nothing outside `xs` names it. A checker
//   that rejected this would ban nested composite literals from every boundary, which
//   would make the deep rule unusable.
//
// RULE CITATION: A1b does not apply: no live alias of the moved storage survives. A4 makes the
//   fresh literal a binding of its own with no link to any outer name.
//
// ---- Tier-0 rule set applied ----
//   Layer 1 (Go-side GC): `xs` ([]holder — moved); the innermost map has no other
//     binding.
//   Layer 2 (Rust-side owned): none.
// ---- Sendability / check sites ----
//   [S1] `task worker(move xs, done)` — an owned move of a self-contained value.
// ---- Expected verdict (hand-derived from VERDICTS.md rule key + SPEC O4/O5
//      BEFORE running the checker) ----
// EXPECTED-VERDICT: ACCEPT
//   ACCEPT — 0 annotations, 0 managed. Pins that the REJECT above tracks the
//   embedded ALIAS at depth 3, not the nesting itself.
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
    xs := []holder{holder{m: map[string]int{"a": 1}}}   // fresh at every level
    done := make(chan bool, 1)
    task worker(move xs, done)                          // self-contained: sendable
    <-done
}
