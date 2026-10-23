// ============================================================================
// cross-fn-field-extraction-launder-reject — the inverse laundering
// direction: instead of smuggling the shared binding INTO a moved composite,
// a helper EXTRACTS the interior reference out of it. `leaked := getm(h)`
// aliases `h.m` (and transitively `shared`); after `move h` the write
// through `leaked` must stay rejected. Pins the extraction side of the
// interprocedural alias summary (return of a field path of a parameter).
//
// ---- Tier-0 rule set applied to each value ----
// Layer 1 (Go-side GC): `shared` aliased by `h.m` (literal edge, corpus 45
//   family) and re-exposed by `getm`'s return; `leaked` is the live alias
//   after the move. `h2` in the worker is the receiver's copy.
// Layer 2/3: none.
//
// ---- Sendability / check sites ----
//   [S1] `task worker(move h)` — moves the composite; freeze must cover the
//        extracted view `leaked` via the call summary.
//   [S2] `leaked["main"]++` — write through the extracted live alias.
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: REJECT A1b
//   REJECT — A1b/E3003 (write through a live alias of a moved value, the
//   alias discovered through a field-path return summary). Without that
//   summary the extraction rebuilds a writable path to storage the move
//   claimed to own.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

struct holder {
    m map[string]int
}

fn getm(h holder) map[string]int {
    return h.m
}

fn worker(h2 holder) {
    h2.m["task"]++
}

fn main() {
    shared := map[string]int{}
    h := holder{m: shared}      // literal edge (corpus 45 family)
    leaked := getm(h)           // extraction hop: leaked aliases h.m
    task worker(move h)         // [S1] spawn site
    leaked["main"]++            // [S2] write through the extracted alias
}
