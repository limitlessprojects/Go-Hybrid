// ============================================================================
// cross-fn-composite-launder-move-reject — interprocedural alias laundering,
// single hop. Pins the alias summary across a function boundary: a composite
// returned by `mk(m)` embeds the caller's live binding `shared`; `move h`
// at the spawn must freeze `shared` through the call boundary, and the
// post-spawn write through `shared` must stay a rejection.
//
// Shape family: 45 (literal embeds live binding) / 54 (field-store edge)
// crossed with a function-return hop — the literal `holder{m: m}` is built
// inside `mk`, so no literal edge exists in `main`; only the interprocedural
// return-argument summary can carry the alias.
//
// ---- Tier-0 rule set applied to each value ----
// Layer 1 (Go-side GC): `shared` is a mutable map written by both tasks.
//   `h := mk(shared)` makes `h.m` alias `shared` transitively through the
//   callee's return expression; O5.2 freeze must reach it.
// Layer 2/3: none.
//
// ---- Sendability / check sites ----
//   [S1] `task worker(move h)` — the moved composite's interior still
//        references the live binding `shared` (interprocedural edge).
//   [S2] `shared["main"]++` after the spawn — write through the laundered
//        live alias.
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: REJECT A1b
//   REJECT — A1b/E3003 (write through a live alias of a moved value). The
//   interprocedural return-argument alias summary is load-bearing here:
//   without it the hop launders the literal edge and this program becomes a
//   silent concurrent map write in accepted code.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

struct holder {
    m map[string]int
}

fn mk(m map[string]int) holder {
    return holder{m: m}
}

fn worker(h holder) {
    h.m["task"]++
}

fn main() {
    shared := map[string]int{}
    h := mk(shared)             // the alias hop: literal edge lives in mk's scope
    task worker(move h)         // [S1] spawn site
    shared["main"]++            // [S2] laundered alias write after the move
}
