// ============================================================================
// 54-field-store-alias-at-spawn — Checker corpus #54 (issue #392)
//
// PURPOSE: negative control for the field-store gap. The alias edge between a
// composite and a live binding was materialized for composite LITERALS
// (`h := holder{m: m}`, corpus 45) and for whole-binding rebinds, but NOT for
// a store into a field (`h.m = m`). Without the edge the composite looks
// self-contained: `move h` neither reports an unproven spawn nor freezes `m`,
// and both writers reach the same map.
//
// ---- Tier-0 rule set applied to each value ----
// Layer 1 (Go-side GC): `shared` is a mutable map read/written by two tasks.
//   `var h holder; h.m = shared` makes `h.m` an alias of `shared`; `move h`
//   must therefore freeze `shared` as a live alias (O5.2).
// Layer 2/3: none.
//
// ---- Sendability / check sites ----
//   [S1] `task worker(move h)` — `h` is moved, but `h.m` still aliases the
//        live binding `shared`.
//   [S2] `shared["main"]++` after the spawn — a WRITE through a frozen alias.
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: REJECT A1b
//   REJECT — A1b/E3003 (write through a live alias of a moved value).
//   Accepting this program is a P0: the emitted Go spawns `worker(h)` while
//   `main` keeps writing `shared`, which is `fatal error: concurrent map
//   writes` at runtime (reproduced 5/5 under go1.26.9).
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

struct holder {
    m map[string]int
}

fn worker(h holder) {
    h.m["task"]++
}

fn main() {
    shared := map[string]int{}
    var h holder
    h.m = shared                    // the alias edge this case pins

    task worker(move h)             // [S1] spawn site

    shared["main"]++                // [S2] alias write after the move
}
