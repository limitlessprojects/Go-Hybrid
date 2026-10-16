// ============================================================================
// 45-composite-literal-embeds-shared-map — Checker corpus #45 (corpus GROWTH,
// added AFTER the checker existed — the "adversarial corpus tests" track of
// BOUNTY.md, not a rewrite of a pre-registered verdict).
//
// (Maintainer integration of PR #176, 2026-10-15: the contributor branched when the corpus
// ended at 31; slots 32-44 were taken by the audit/PR wave, so this lands as #45. Payload, rule
// analysis, and verdict line unchanged — verified REJECT A1c (HYB-E3001) against the current
// checker before the merge.)
//
// PURPOSE: pin O4.6 deep sendability for a fresh COMPOSITE LITERAL that
// embeds a live shared mutable map at a SPAWN site. GUARANTEES.md claim #3
// ("Composites ... embedding an alias does not launder it") is today pinned
// only by:
//   - 01 — the bare map crossing a spawn directly (no composite);
//   - 03 — a struct shared by POINTER (`&stats`), i.e. an address-of, not a
//          value literal that wraps the alias;
//   - the SPEC §9.3 O4.6 prose example, which is a SEND site
//          (`jobs <- job{data: shared_map}` → HYB-E3002).
// No corpus case exercises the other half of the claim: a fresh value
// literal (slice / struct / nested map) that carries a live mutable-map
// alias across a `task` SPAWN (HYB-E3001). This is the "wrap it in a fresh
// `[]T{...}` and it looks new" shape — the one O4.6 exists to stop from
// being lucky rather than sound. classify_composite walks every part and
// the first live-mutable part rejects the whole literal; this case pins
// that walk at the spawn check-site.
//
// ---- Tier-0 rule set applied to each value ----
// Layer 1 (Go-side GC): `shared` (map[string]int — MUTABLE, aliased by the
//   literal AND still named in main), `batch` (the fresh slice literal —
//   not laundering: its element [0] IS `shared`), `done` (channel —
//   safe-by-type), `i` (int copy).
// Layer 2 (Rust-side owned): none.
// Layer 3 (boundary handles): none.
//
// ---- Sendability / check sites (task spawn / channel send) ----
//   [S1] `task worker([]map[string]int{shared}, done)`:
//        done — channel: sendable (A1c safe-by-type).
//        `[]map[string]int{shared}` — a FRESH slice literal (class-5
//        candidate), BUT its reference-typed element is `shared`, a live
//        mutable GC map that is neither moved, managed, nor channel-typed.
//        O4.6: a fresh literal is sendable only when every reference-typed
//        part is sendable. The part is not → the whole literal is NOT
//        SENDABLE. VIOLATION (A1c), reported at the literal, before the
//        worker body's writes matter.
//   [S2] `done <- true` — bool literal: sendable (all readings).
//   The `m["k"] = i` writes in the worker AND `shared["k"] = i` in main are
//   a real concurrent-map-write data race (Go runtime fatal) — the verdict
//   is decided at [S1] first, exactly as in 01/03.
//
// ---- Sibling shapes (same rule, documented so coverage is explicit) ----
//   S3 struct wrap:   task worker2(wrap{rows: shared}, done)   // REJECT A1c (O4.6)
//   S4 nested map:    task worker3(map[string][]map[string]int{"g": batch}, done) // REJECT A1c (O4.6)
//   Each rejects at the first live-mutable reference-typed part; only the
//   active [S1] program carries the pinned verdict line below.
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: REJECT A1c
//   REJECT — A1c via the O4.6 deep-sendability path (HYB-E3001 at the spawn,
//   "live mutable value referenced by the fresh literal"). NOT contested:
//   wrapping a shared mutable map in a fresh `[]T{...}` does not create a
//   new, unaliased value — element [0] is the same map main still holds, so
//   the race the strict ruling exists to reject is fully present. Legal
//   routes for the diagnostic: `move` the map into the literal
//   (`[]map[string]int{move shared}` once main is done with it), the
//   `managed(...)` hatch (as 02/18), or per-task fresh maps merged over a
//   channel (15).
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0 — as written it needs one
//   (or a redesign); that is the tripwire measurement, not a corpus defect.
//   Checker-verdict test, not an execution test: no runtime output pinned.
// ============================================================================

fn worker(batch []map[string]int, done chan<- bool) {
    m := batch[0]
    for i in 0..1000 {
        m["k"] = i                     // shared-map write through the slice
    }                                  // element (Go: concurrent-write fatal;
    done <- true                       // Hybrid: rejected at spawn site [S1])
}

fn main() {
    shared := map[string]int{"k": 0}
    done := make(chan bool, 1)

    task worker([]map[string]int{shared}, done)  // SPAWN SITE [S1] — VIOLATION
                                                 // (A1c / O4.6): the fresh slice
                                                 // literal embeds `shared`, a live
                                                 // mutable GC map (not moved,
                                                 // managed, or channel-typed)
    for i in 0..1000 {
        shared["k"] = i                // concurrent write from main — the race
    }

    <-done
}
