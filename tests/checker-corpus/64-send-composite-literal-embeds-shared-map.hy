// ============================================================================
// 64-send-composite-literal-embeds-shared-map -- Checker corpus #64 (corpus
// GROWTH: an adversarial corpus test, the BOUNTY.md "useful improvements"
// track, not a rewrite of a pre-registered verdict).
//
// Registered as 64: the PR was numbered 56 when written, but 56-63 were taken
// by the defer/shadowing waves before it landed, so it is renumbered to the
// next free number at integration time (maintainer integration of PR #451).
//
// PURPOSE: pin O4.6 deep sendability at the CHANNEL SEND check-site for a
// fresh COMPOSITE LITERAL that embeds a live shared mutable map.
//
// The claim is already pinned on the other three sides:
//   - 01  -- a bare shared map crossing a `task` SPAWN directly;
//   - 03  -- a struct shared by POINTER (`&stats`) at a spawn;
//   - 45  -- a fresh slice literal wrapping a shared map at a SPAWN
//            (`task worker([]map[string]int{shared}, done)` -> HYB-E3001);
//   - 54  -- `h.m = shared` materializing may_share on a field store, at a spawn;
//   - the SPEC section 9.3 O4.6 PROSE example, which is a send
//            (`jobs <- job{data: shared_map}` -> HYB-E3002).
//
// The one shape with no corpus file behind it is exactly the pair of those
// last two: a fresh composite literal wrapping a live shared map at a plain
// channel SEND. 45's own header names the omission -- "the SPEC section 9.3
// O4.6 prose example, which is a SEND site". Neither 32 nor 34 covers it:
// both are `select` arm heads, 32 a bare shared map, 34 an unproven name. A
// checker that walks composites at a spawn but classifies the send operand by
// shape alone would pass every case in the corpus today and fail this one, so
// this fixture is the cheapest way to hold section 8.4 C5 and 45 to the same
// O4.6 walk.
//
// ---- Tier-0 rule set applied to each value ----
// Layer 1 (Go-side GC): `shared` (map[string]int -- MUTABLE, aliased by the
//   literal AND still named in main), `jobs` (chan []map[string]int -- safe
//   by type, O4 class 2: the value crossing the site would be the channel,
//   but the channel is not what is being sent).
// Layer 2 (Rust-side owned): none.
// Layer 3 (boundary handles): none.
//
// ---- Sendability check-site ----
//   [S1] `jobs <- []map[string]int{shared}`:
//        jobs -- channel: safe-by-type, but a `make(...)` binding is the
//                send TARGET, not a send operand; it is not classified.
//        `[]map[string]int{shared}` -- a FRESH slice literal (class-5
//        candidate) whose only reference-typed element IS `shared`, a live
//        mutable map still named in main. O4.6 deep sendability: the literal
//        does not launder the alias. NOT SENDABLE. VIOLATION (A1c).
//   [S2] `close(jobs)` -- a close, not a sendability check-site.
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: REJECT A1c
//   REJECT -- A1c. Not contested: an unsynchronized map is written by main
//   and handed to whoever later receives from `jobs`, which is the shape
//   reading (c) exists to reject, and the Go runtime fatals on concurrent
//   map writes anyway -- the strict verdict agrees with the runtime, earlier.
//   Deliberately NOT asserted: an EXPECTED-DIAGNOSTIC line. Both HYB-E3002
//   (send-site, section 8.4 C5) and HYB-E3001 would be defensible readings
//   of which check-site fires first, and the corpus should pin the verdict
//   and the rule, not adjudicate that ordering here.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

fn main() {
    shared := map[string]int{}
    shared["a"] = 1
    jobs := make(chan []map[string]int, 1)
    jobs <- []map[string]int{shared}
    close(jobs)
}
