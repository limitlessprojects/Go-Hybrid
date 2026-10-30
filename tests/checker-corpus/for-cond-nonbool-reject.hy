// ============================================================================
// for-cond-nonbool-reject -- issue #1032
//
// PURPOSE: pin the Go-style condition loop (`for cond {}`, ForKind::While)
// type check. The condition must be boolean; `for i { }` with an int `i` is
// a build-time error in Go and must be one here (GUARANTEES row 13,
// BUILDABLE). The check landed in buildable.rs (check_bool_condition) via the
// Wave-2 round; this file pins the repro. Companion void-call work is in
// open PR #1038 -- this fixture only covers the non-boolean condition seam.
//
// ---- Tier-0 rule set applied to each value ----
// Layer 1: none (no values cross a boundary).
// Layer 2/3: none.
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: REJECT BUILDABLE
//   REJECT -- HYB-E1002 (non-boolean condition in a `for` statement).
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

fn main() {
    i := 0
    for i {
        i = i + 1
    }
}
