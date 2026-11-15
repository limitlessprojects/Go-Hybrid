// ============================================================================
// 10-a5-http-handler-capture — Checker corpus #10 (pre-registered BEFORE
// any checker exists; human ruling 2026-10-12, QUESTIONS.md Q1).
//
// PURPOSE: exercise A5 (the Go-API escape perimeter) on the shape that
// forces the decision: a MUTABLE GC value CAPTURED by a handler closure
// handed to foreign Go code (net/http), which may run it on other
// goroutines (one per request in production). There is NO Hybrid
// spawn/send site anywhere — the value escapes through the Go API instead.
//
// DECIDED (W-016 M2/M3): the A5 perimeter is a FOURTH CHECK-SITE class.
// SPEC-LANGUAGE §8.5 C9 was promoted from the scoped claim to a hard,
// closed-world rule with no curated Go API database: a closure that writes
// a captured mutable binding and is passed to a package-qualified foreign
// Go call must take `managed("...", "interop")` at the call site, or the
// capture must be `const`/non-`may_share`. This program has neither, so
// it FLIPS from ACCEPT to REJECT (HYB-E3013, tier-0 rule A5). The flip is
// logged here rather than silent — pre-registration is amended in place.
//
// HISTORY: pre-registered 2026-10-12 as ACCEPT (CONTESTED) under the old
// scoped-claim ruling, with an explicit "flips to REJECT if the fourth
// check-site class lands" clause. W-016 is that landing. This is
// request-counter middleware; every Go service has one, which is exactly
// why the rule earns its keep.
//
// ---- Tier-0 rule set applied to each value ----
// Layer 1 (Go-side GC): `hits` (int — MUTABLE GC value captured by the
//   closure), `mux`, the handler closures themselves (GC closures;
//   conversion to http.handler is Go interop — spike-6 06), `w`/`r`,
//   `code`/`body` (value copies).
// Layer 2 (Rust-side owned): none.
// Layer 3 (boundary handles): none — net/http + httptest are GO packages;
//   Hybrid→Go interop is call-level after Go-source emission (Spike 2),
//   NOT the checked Rust boundary.
//
// ---- Sendability / check sites (task spawn / channel send) ----
//   NONE in Hybrid code (no `task`, no channel send). PERIMETER: the
//   `hits` capture in the closure passed to `http.handler(...)` (and that
//   value handed to `mux.handle(...)`) escapes into Go code — this is the
//   HYB-E3013 / A5 check-site (W-016 M3).
//
// ---- Expected verdict (amended 2026-10-29 by W-016 M3) ----
// EXPECTED-VERDICT: REJECT A5
//   REJECT with HYB-E3013 (tier-0 rule A5): the closure writes the captured
//   mutable binding `hits` and is passed to the foreign Go call
//   `http.handler(...)`. HONESTY NOTE: production net/http serves handlers
//   concurrently — `hits++` from two overlapping requests is a REAL Go data
//   race, which is why this shape is now a hard reject rather than an
//   accepted escape. The fix is `managed("counter", "interop")` (or an
//   atomic/const capture).
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

import "net/http"

fn main() {
    hits := 0                    // mutable GC value, captured below

    mux := http.new_serve_mux()
    mux.handle("/ping", http.handler(fn(w http.response_writer, r *http.request) {
        hits++                   // runs on Go's request goroutines — the A5
        fmt.fprintln(w, "pong {hits}")  // escape perimeter, not a check site
    }))

    // in-process smoke client (sequential, as in spike-6 06)
    for _ in 0..3 {
        code, body := httptest.get(mux, "/ping")?
        fmt.println("{code} {body}")
    }
}
