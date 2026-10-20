// ============================================================================
// 51-make-slice-cap.hy — Checker corpus #51 (added WITH the #353 fix: the
// 3-argument slice form of `make` was rejected as HYB-E2005 before it; it is
// valid Go and is accepted after it).
//
// PURPOSE: pin the slice-only 3-argument `make` form — `make([]T, len, cap)`
// — as ACCEPT, and pin that the spare capacity it produces is an ordinary
// fresh local for Tier-0 purposes (no check-site, no loan, no sharing). The
// same arity on a map stays HYB-E2005 (unit tests in
// hy-check/tests/builtin_args_defer.rs); this file is the accepted half.
//
// ---- Tier-0 rule set applied ----
// Layer 1 (Go-side GC): `s` — a fresh local slice with spare capacity; it
//   never crosses a spawn or send site, so no sendability class applies.
//   The `s[0] = 1` store is a single-task write to a local: legal.
// Layer 2 (Rust-side owned): none.
// Layer 3 (boundary handles): none.
//
// ---- Expected verdict (registered with the fix, 2026-10-17) ----
// EXPECTED-VERDICT: ACCEPT
//   ACCEPT — valid Go, zero check-sites. A checker that rejects this is
//   over-strict (the corpus contract's bug side), and the diagnostic that
//   said "Go rejects this builtin shape" was wrong for the slice form.
//
// ---- Annotations / managed ----
// lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

import "fmt"

fn main() {
    s := make([]int, 3, 10)
    s[0] = 1
    fmt.println("len {len(s)} cap {cap(s)}")
}
