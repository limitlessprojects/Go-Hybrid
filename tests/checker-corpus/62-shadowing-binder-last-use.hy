// ============================================================================
// 62-shadowing-binder-last-use — Checker corpus #62 (additive registration,
// 2026-10-18, issue #547).
//
// PURPOSE: pin the O8 non-lexical last-use window when a LATER BINDER reuses
// the loan's name. `n := &m["a"]` opens a unique map-slot loan on `n`; its
// last use is `n.x = 1`, BEFORE `task worker(move m, done)`. The
// `for n in 0..1000` binder re-declares `n`, so every `n` inside the loop is
// the LOOP variable, never the loan. The program is therefore sound: after the
// move, `main` never touches `m`.
//
// This case is an ACCEPT pin plus an EMITTER gate. Be precise about WHY it
// accepts — the two halves of the pipeline disagree here, deliberately:
//   * SOURCE semantics: the loan's last use is `n.x = 1`, before the move.
//   * CHECKER: hy-check's `FlatInfo::last_use` is keyed by NAME, so it takes
//     `last_use["n"]` from the shadowed body's `total += n`; its L1 window
//     runs PAST `move m`. The program still ACCEPTs because no rule consumes
//     a map-slot window at a `move` — loans.rs emits only E3101/E3102/E3103/
//     E3104, and none of them is "move while a slot loan is live".
//   * EMITTER: the store-back `m["a"] = n` must be flushed BEFORE the spawn.
//     Before the #547 fix the loop's `n` was counted as a use of the loan, the
//     store-back was deferred past `go worker(m, done)`, and `go run -race`
//     reported a DATA RACE in 3/3 runs (the moved map written by both parties).
//
// ---- Tier-0 rule set applied to each value ----
// Layer 1 (Go-side GC): `m` (map[string]item — arena + GC), `n` (the *item
//   map-slot loan), `done` (channel), `total`/`i` (ints).
// Layer 2 (Rust-side owned): none.
// Layer 3 (boundary handles): `done` (channel — sendable by class 2).
//
// ---- Loans traced (non-lexical windows) ----
//   L1 = `n := &m["a"]` — UNIQUE map-slot loan, created at statement 2.
//   SOURCE last use: `n.x = 1` (statement 3); the shadowing `for` binder at
//   statement 6 does NOT extend it — the loop's `n` is a different binding
//   (issue #547). No map insert/delete falls inside the source window.
//   EMITTER window (what #551 fixes): the non-lexical store-back must land at
//   `n.x = 1`, BEFORE `task worker(move m, done)` — pinned by
//   hy-emit/src/tests.rs `slot_loan_flushes_before_move_when_*_shadows_*`.
//
// ---- KNOWN, NOT FIXED HERE (fail-closed) ----
//   A map insert INSIDE the shadowing loop body is reported HYB-E3102, because
//   the checker's name-keyed window still covers the body. That over-rejects;
//   tightening it needs binding-identity tracking in hy-check (a naive "skip
//   shadowed uses" rule would collapse a nested same-name loan's window to its
//   creation point and turn a real E3102 into a silent ACCEPT).
//
// ---- Sendability / check sites (task spawn) ----
//   `task worker(move m, done)` — `move m` transfers the map (O4 owned-move
//   route); the loan L1 is already closed, so nothing aliases `m` after the
//   spawn. `done` is a channel (class 2).
//
// ---- Expected verdict (registered 2026-10-18, issue #547) ----
// EXPECTED-VERDICT: ACCEPT
//   ACCEPT — the source is sound; `move m` is the loan's last touch of the
//   arena. This ACCEPT must survive any future tightening of the checker's
//   last-use window (that is why it is registered), and an emitter that defers
//   the store-back past the spawn reintroduces the #547 data race.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

struct item {
    x int
}

fn worker(m map[string]item, done chan bool) {
    for i in 0..1000 {
        m["w"] = item{x: i}
    }
    done <- true
}

fn main() {
    m := map[string]item{"a": item{x: 0}}
    done := make(chan bool, 1)
    n := &m["a"]                    // LOAN L1: map-slot borrow (creation)
    n.x = 1                         // L1 LAST USE: window ends here
    task worker(move m, done)       // the arena leaves main; L1 is already closed
    total := 0
    for n in 0..1000 {              // SHADOWING BINDER: this `n` is the loop
        total += n                  // variable — NOT a use of L1 (#547)
    }
    <-done
    fmt.println(total)
}
