// ============================================================================
// 63-shadowing-binders-at-spawn — Checker corpus #63 (additive registration,
// 2026-10-18, issue #547 follow-up).
//
// PURPOSE: the two binder shapes that #551's first cut MISSED, in one program,
// behind a single move boundary:
//   * an `if` INIT that declares the loan's name (`if n := 2; n > 1`), whose
//     scope covers `cond`, both arms and `else` (Go's if-statement scope);
//   * a closure NAMED RESULT that declares it (`fn() (n int) { n = 7 }`),
//     whose scope is the closure body.
//
// Both keep the loan's real last use at `n.replicas = 5`, BEFORE
// `task worker(move m, done)`. Counting the shadowed mentions as uses deferred
// the store-back `m["a"] = n` past the spawn — the same data race as #547, and
// `hyb check` still reported 0 diagnostics. This case pins the source as
// ACCEPT and (through tools/differential_loop.py, which builds and runs it)
// that the emitted program is still valid Go after the flush moves earlier.
//
// ---- Tier-0 rule set applied to each value ----
// Layer 1 (Go-side GC): `m` (map[string]service — arena + GC), `n` (the
//   *service map-slot loan), `done` (channel), `total`/`k` (ints).
// Layer 2 (Rust-side owned): none.
// Layer 3 (boundary handles): `done` (channel — sendable by class 2).
//
// ---- Loans traced (non-lexical windows) ----
//   L1 = `n := &m["a"]` — UNIQUE map-slot loan. SOURCE last use:
//   `n.replicas = 5`, before the move. The `if` init and the closure's named
//   result both RE-DECLARE `n`, so neither body's `n` is a mention of L1
//   (issue #547). No map insert/delete falls inside the window.
//   EMITTER window: the store-back must land before the spawn — pinned by
//   hy-emit/src/tests.rs `slot_loan_flushes_before_move_when_if_init_shadows_*`
//   and `..._closure_named_result_shadows_*`.
//
// ---- Sendability / check sites (task spawn) ----
//   `task worker(move m, done)` — `move m` transfers the arena; L1 is closed.
//
// ---- Expected verdict (registered 2026-10-18) ----
// EXPECTED-VERDICT: ACCEPT
//   ACCEPT — sound source: after the move, `main` never touches `m`; `n` in
//   the if and in the closure is a different binding in both cases.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

struct service {
    replicas int
}

fn worker(m map[string]service, done chan bool) {
    done <- true
}

fn main() {
    m := map[string]service{"a": {replicas: 1}}
    n := &m["a"]                    // LOAN L1: map-slot borrow (creation)
    n.replicas = 5                  // L1 LAST USE: window ends here
    done := make(chan bool, 1)
    task worker(move m, done)       // the arena leaves main; L1 is already closed
    total := 0
    if n := 2; n > 1 {              // SHADOWING INIT: this `n` is the if-local
        total += n
    }
    f := fn() (n int) {             // SHADOWING NAMED RESULT: this `n` is the
        n = 7                       // closure's result, not the loan
        return
    }
    total += f()
    <-done
    fmt.println(total)
}
