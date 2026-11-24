// ============================================================================
// 1997-any-field-store-controls-accept — Checker corpus (issue #1997:
// `any`-field composites drop non-owned/may-share edges).
//
// PURPOSE: the ACCEPT-side control for the reject twin: a SCALAR stored
//   into the same `any`-typed struct field carries no shared storage, so
//   the move stays exclusive and the program accepts. Guards the fix
//   against over-strictness — a checker that rejects ANY store into an
//   `any` field (treating the erasure itself as may-share) fails this
//   program, and that would be a bug in the checker, not the corpus.
//
// RULE CITATION: O5.2 / A1b do not apply: `1` is a scalar value, not a
//   reference to mutable GC storage, so there is no alias for the freeze
//   to cover.
//
// ---- Tier-0 rule set applied ----
//   Layer 1 (Go-side GC): `h` (holder value, moved), `h.m` (any field
//     holding the scalar `1`).
//   Layer 2 (Rust-side owned): none.
//   Layer 3 (boundary handles): none.
//
// ---- Sendability / check sites ----
//   [S1] `h.m = 1` — scalar store into an `any` field: no edge.
//   [S2] `task worker(move h)` — the move is exclusive; nothing is
//        aliased.
//
// ---- Expected verdict (hand-derived from VERDICTS.md A1b + O5.2 BEFORE
//      running the checker) ----
// EXPECTED-VERDICT: ACCEPT
//   ACCEPT — no shared storage crosses the move; the scalar control
//   pins the fix's non-over-strict side.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

struct holder { m any }

fn worker(h holder) { h.m = h.m }

fn main() {
    var h holder
    h.m = 1                     // [S1] scalar into an any field: no edge
    task worker(move h)         // [S2] exclusive move
    fmt.println("ok")
}
