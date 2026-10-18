// ============================================================================
// chan-of-closure-at-send — Checker corpus (bounty track: "grow the adversarial corpus").
//
// PURPOSE: a CHANNEL OF CLOSURES (`chan fn()`, 0/60) — and the narrower question the
//   corpus never asks: is a closure that captures NOTHING still refused at a
//   boundary? It is. This fixture is deliberately capture-free so the answer
//   cannot be confused with the capture rules (36, 50, 25).
//
// RULE CITATION: A1c (safe-by-type): a closure is not a channel/atomic/immutable value, is not
//   `move`d and is not inside `managed`. The A5 perimeter is the reason — a
//   closure value carries an environment, so "sendable" would have to mean the
//   environment is sendable, which the checker cannot prove from the type.
//
// ---- Tier-0 rule set applied ----
//   Layer 1 (Go-side GC): `f` (a closure over an empty environment), `ch`.
// ---- Sendability / check sites ----
//   [S1] `ch <- f` — VIOLATION: a closure value at a send site, with no `move`.
//        The capture-free body is intentional: the rejection is about the VALUE
//        CLASS, not about what this particular closure captured.
// ---- Expected verdict (hand-derived from VERDICTS.md rule key + SPEC O4
//      BEFORE running the checker) ----
// EXPECTED-VERDICT: REJECT A1c
// EXPECTED-DIAGNOSTIC: HYB-E3002
//   REJECT — A1c. Note the asymmetry with a declared top-level `fn`, which IS
//   spawnable (corpus 25's control): a top-level fn has no environment to
//   share, a closure value does.
//   CONTESTED: the rule as written rejects this, and the checker agrees; if the
//   maintainer rules the shape should be legal, this row moves to ACCEPT by a
//   logged amendment (VERDICTS.md) — it is registered as REJECT because that is
//   what the rule key says today.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

fn main() {
    f := fn() { fmt.println("hi") }
    ch := make(chan fn(), 1)
    ch <- f            // [S1] VIOLATION: a closure value is not sendable-by-type
    fmt.println("done")
}
