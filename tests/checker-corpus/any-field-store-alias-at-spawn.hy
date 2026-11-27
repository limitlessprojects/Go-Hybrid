// ============================================================================
// any-field-store-alias-at-spawn — Checker corpus (issue #4755)
//
// PURPOSE: negative control for the `any`-typed field-store gap. Corpus 54
// (`54-field-store-alias-at-spawn.hy`) rejects `h.m = shared` when `m` is
// declared `map[string]int`, but the same store was ACCEPTED when `m` was
// declared `any`: the erased field shape (`Unknown`) made the struct look
// non-sharing, so `move h` never froze `shared`.
//
// `any` can hold a map, slice, or pointer, so a struct carrying an `any`
// field may share storage exactly like the concretely-typed twin.
//
// ---- Tier-0 rule set applied ----
//   Layer 1 (Go-side GC): `shared` is a mutable map read/written by two
//     tasks. `var h holder; h.m = shared` makes `h.m` an alias of `shared`;
//     `move h` must freeze `shared` as a live alias (O5.2).
//
// ---- Sendability / check sites ----
//   [S1] `task worker(move h)` — `h` is moved while `h.m` aliases `shared`.
//   [S2] `shared["main"]++` — a WRITE through a frozen alias.
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: REJECT A1b
// EXPECTED-DIAGNOSTIC: HYB-E3003
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

struct holder {
    m any
}

fn worker(h holder) {
    h.m = h.m
}

fn main() {
    shared := map[string]int{}
    var h holder
    h.m = shared                    // the alias edge this case pins

    task worker(move h)             // [S1] spawn site

    shared["main"]++                // [S2] alias write after the move
}
