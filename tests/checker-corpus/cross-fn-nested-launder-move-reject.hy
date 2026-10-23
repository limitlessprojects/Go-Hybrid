// ============================================================================
// cross-fn-nested-launder-move-reject — interprocedural alias laundering,
// two hops. `outer(shared)` forwards into `inner(m)` which builds the
// composite. Pins that the return-argument alias summary is transitive
// through nested calls, not just a single direct hop (companion of
// cross-fn-composite-launder-move-reject).
//
// ---- Tier-0 rule set applied to each value ----
// Layer 1 (Go-side GC): `shared` is a mutable map written by both tasks;
//   the alias path is main -> outer -> inner -> literal -> h.m.
// Layer 2/3: none.
//
// ---- Sendability / check sites ----
//   [S1] `task worker(move h)` — moved composite, interior aliases `shared`
//        through two call frames.
//   [S2] `shared["main"]++` — write through the laundered live alias.
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: REJECT A1b
//   REJECT — A1b/E3003. If the summary were only one frame deep, the second
//   hop would launder the first and this becomes accepted-but-racy.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

struct holder {
    m map[string]int
}

fn inner(m map[string]int) holder {
    return holder{m: m}
}

fn outer(m map[string]int) holder {
    return inner(m)
}

fn worker(h holder) {
    h.m["task"]++
}

fn main() {
    shared := map[string]int{}
    h := outer(shared)          // two-frame alias hop
    task worker(move h)         // [S1] spawn site
    shared["main"]++            // [S2] laundered alias write
}
