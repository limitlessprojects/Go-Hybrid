// ============================================================================
// 47-loop-call-local-backedge — Checker corpus #47 (corpus GROWTH, added AFTER
// the checker existed — the "adversarial corpus tests" track of BOUNTY.md).
//
// (Numbering note: #195's fix, in flight as PR #271, registers a row 46. This
// row is 47 so that both can land without a collision; renumber freely if you
// take them in the other order.)
//
// PURPOSE: pin the loop-backedge half of the alias freeze for a local that a
// CALL initialized (issue #194, follow-up gap in #23's fix). `check_loop_backedge`
// exempts loop-local bindings initialized by a call from the loop-carried
// reset, because the checker has no interprocedural summary and must not
// reject the fresh-copy control (corpus 15's `chunk_of` returns
// `append([]string{}, …)`). The exemption keyed on `resolve_alias(st, lhs)` —
// the OUTER name — so for `m := same(agg)` it exempted `agg` itself and erased
// the loop-carried move, letting every iteration move the same map again.
//
// THE PROGRAM (from issue #194, verbatim):
//
//   fn same(m map[string]int) map[string]int { return m }
//   ...
//   agg := map[string]int{"k": 0}
//   for i in 0..2 {
//       m := same(agg)              // aliases agg — `same` returns its param
//       task worker(move m, done)   // iteration 1 moves agg's storage
//   }                               // iteration 2 reads the moved agg again
//
// The exemption now only assumes a FRESH result when the callee is not known
// to return one of its own parameters (`Checker::callee_returns_own_param`,
// same-file summaries). `same` does, so `agg` stays loop-carried-moved and the
// second iteration's read is rejected. Corpus 15's `chunk_of` is unaffected —
// its returns are a fresh `append`, so the exemption still fires there.
//
// THE BUG (before the fix, on c09e9928): `hyb check` printed
// `OK (0 diagnostics)`; the reporter's `--emit-go` + `go run -race` run on the
// same program reported a data race between two `main.worker` goroutines on
// the same map, then `fatal error: concurrent map writes`, exit 2 (issue #194
// evidence, reproduced by the reporter on darwin-arm64; this fixture pins the
// check-side verdict only).
//
// UNCONTESTED: GUARANTEES.md claims #1 (spawn args) and #6 (move/freeze),
// SPEC §9.3 O4/O5, §9.2. Across the back edge `agg` is moved (through the
// alias `m`) and read again — the E3003-class use-after-move the loop-backedge
// check exists for.
//
// ---- Tier-0 rule set applied to each value ----
// Layer 1 (Go-side GC): `agg` (map[string]int — MUTABLE, aliased by `m`),
//   `done` (channel), and the per-task `m` binding.
// Layer 2 (Rust-side owned): none.
// Layer 3 (boundary handles): none.
//
// ---- Sendability / check sites (task spawn / channel send) ----
//   [S1] `m := same(agg)` — a call-initialized loop-local whose callee returns
//        its own parameter, so the recorded alias is REAL.
//   [S2] `task worker(move m, done)` — OWNED MOVE (A1c route 2); the A1b
//        freeze reaches `agg` because `m` aliases it.
//   [S3] `done <- true` in worker — sendable.
//
// ---- Expected verdict (registered 2026-10-16, before running the fixture
// against the fixed checker) ----
// EXPECTED-VERDICT: REJECT A1b [HYB-E3003]
//
// The second iteration re-reads `agg` at `m := same(agg)` after the move; the
// checker must reject that (E3003) rather than erase the loop-carried move.
// ============================================================================

fn same(m map[string]int) map[string]int {
    return m
}

fn worker(m map[string]int, done chan<- bool) {
    for i in 0..100000 {
        m["k"] = i
    }
    done <- true
}

fn main() {
    agg := map[string]int{"k": 0}
    done := make(chan bool, 10)
    for i in 0..2 {
        m := same(agg)
        task worker(move m, done)
    }
    for i in 0..2 {
        <-done
    }
}
