// ============================================================================
// 46-shadow-may-share-freeze — Checker corpus #46 (corpus GROWTH, added AFTER
// the checker existed — the "adversarial corpus tests" track of BOUNTY.md).
//
// PURPOSE: pin the may-share half of the block-shadow freeze rule (issue
// #195, follow-up gap in the #14 / #31 fixes). Corpus 40/41 pin the WRAPPER
// tail of the alias collector (a nested call return); 42/43 pin the
// copy/projection tail. None of them exercises the case where the moved
// binding's storage peer is reachable only through `may_share`: a struct
// that EMBEDS a reference.
//
// THE PROGRAM (from issue #195, verbatim):
//
//   struct holder { m map[string]int }
//   ...
//   m := map[string]int{"k": 0}
//   h := holder{m: m}
//   if true {
//       m := 42                      // shadow the outer map binding
//       task worker(move h, done)    // moves h — h.m is the SAME map as m
//   }
//   for i in 0..100000 { m["k"] = i }   // <-- must be frozen by the move
//
// `h := holder{m: m}` links `h` and `m` through `may_share`, NOT through
// `aliases`: an embedded reference is not a reference alias (the binding `h`
// is a struct value; only its field carries the map). Two interacting gaps
// made the outer write pass with 0 diagnostics:
//
//   1. `mark_move` has to freeze every storage peer of the moved binding, and
//      it did walk `share_group` — but the freeze is recorded on the binding
//      the NAME resolves to at this depth, and inside the block `m` resolves
//      to the inner `m := 42` (a primitive). The outer map was never touched.
//   2. On block exit `scope_pop` propagated the freeze across the restored
//      entries of `self.aliases` only. The `h <-> m` edge lives in
//      `self.may_share`, so the restored outer `m` never inherited the move.
//
// The fix sweeps BOTH maps in `scope_pop` (inner-move capture and the
// restored-outer propagation) and forwards the freeze to the whole may-share
// component of the moved binding and of its alias root. The shadow dies at
// the pop, so `env_lookup_mut("m")` then reaches the OUTER map binding.
//
// THE BUG (before the fix, on c09e9928): `hyb check` printed
// `OK (0 diagnostics)`; the reporter's `--emit-go` + `go run -race` run on the
// same program reported a data race between the main goroutine and
// `main.worker` on the same map, then `fatal error: concurrent map writes`,
// exit 2 (issue #195 evidence, reproduced by the reporter on darwin-arm64;
// this fixture pins the check-side verdict only).
//
// UNCONTESTED: GUARANTEES.md claim #3 ("embedding an alias does not launder
// it") and claim #6 (move/freeze), SPEC §9.3 O4.6/O5. Moving `h` must freeze
// every share peer of `h` — including the outer `m` that `h.m` aliases. The
// inner `m := 42` is a fresh primitive binding and is irrelevant to the
// outer map's ownership.
//
// ---- Tier-0 rule set applied to each value ----
// Layer 1 (Go-side GC): `m` (map[string]int — MUTABLE, aliased by `h.m`),
//   `done` (channel), receiver-side `h` (owned after the move; its `m` field
//   is the same map value).
// Layer 2 (Rust-side owned): none.
// Layer 3 (boundary handles): none.
//
// ---- Sendability / check sites (task spawn / channel send) ----
//   [S1] `task worker(move h, done)` — OWNED MOVE of `h` (A1c route 2). The
//        A1b freeze starts HERE and must cover the binding AND every storage
//        peer, including the outer `m` via `may_share` (this fixture).
//   [S2] `done <- true` in worker — sendable.
//
// ---- Expected verdict (registered 2026-10-16, before running the fixture
// against the fixed checker) ----
// EXPECTED-VERDICT: REJECT A1b [HYB-E3003]
//
// The write at `m["k"] = i` after the block is a WRITE through a live alias of
// a moved value; the checker must reject it (or reject the spawn). The
// diagnostic is the same E3003 family as 40/43.
// ============================================================================

struct holder {
    m map[string]int
}

fn worker(h holder, done chan<- bool) {
    for i in 0..100000 {
        h.m["k"] = i
    }
    done <- true
}

fn main() {
    m := map[string]int{"k": 0}
    h := holder{m: m}
    done := make(chan bool, 1)
    if true {
        m := 42
        fmt.println(m)
        task worker(move h, done)
    }
    for i in 0..100000 {
        m["k"] = i
    }
    <-done
}
