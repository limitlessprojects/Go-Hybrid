// ============================================================================
// 1997-any-field-store-reject — Checker corpus (issue #1997: `any`-field
// composites drop non-owned/may-share edges).
//
// PURPOSE: a SHARED MAP stored into an `any`-typed struct field must still
//   materialise the alias edge. Before the fix the store gate only
//   consulted the ROOT struct's declared shape, and `shape_may_share`
//   returns false when the only reference-carrying field is `any` /
//   `TypeShape::Unknown`, so `task worker(move h)` froze only `h` and the
//   later `shared["main"]++` was accepted — contradicting GUARANTEES.md
//   O5.2 (the freeze covers every live alias of the moved value).
//
// RULE CITATION: A1b + O5.2: `h.m` holds the same map header as `shared`.
//   Moving `h` freezes that storage, so the later `shared["main"]++` is a
//   write through a live alias of a moved value.
//
// ---- Tier-0 rule set applied ----
//   Layer 1 (Go-side GC): `h` (holder value, moved), `h.m` (any field
//     holding the map header), `shared` (the original map binding).
//   Layer 2 (Rust-side owned): none.
//   Layer 3 (boundary handles): none.
//
// ---- Sendability / check sites ----
//   [S1] `h.m = shared` — the field store that must record the edge.
//   [S2] `task worker(move h)` — the move freezes the aliased storage.
//   [S3] `shared["main"]++` — write through the live alias after the
//        freeze. VIOLATION (A1b).
//
// ---- Expected verdict (hand-derived from VERDICTS.md A1b + O5.2 BEFORE
//      running the checker) ----
// EXPECTED-VERDICT: REJECT A1b
//   REJECT — A1b. Storing a shared map into an `any` field does not
//   launder the alias: the move freezes the map the sender still holds.
// EXPECTED-DIAGNOSTIC: HYB-E3003
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

struct holder { m any }

fn worker(h holder) { h.m = h.m }

fn main() {
    shared := map[string]int{}
    var h holder
    h.m = shared                 // [S1] field store into an any field
    task worker(move h)          // [S2] move freezes the aliased storage
    shared["main"]++             // [S3] write through a live alias — A1b
}
