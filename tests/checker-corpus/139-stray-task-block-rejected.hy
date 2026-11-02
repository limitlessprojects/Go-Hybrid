// ============================================================================
// 139-stray-task-block-rejected — Auto scheduling corpus #139 (issue #1962).
//
// PURPOSE: negative control — `task { ... }` written with NO enclosing
// `auto { ... }`. The inline block form only means something to the
// scheduler: it is the unit the region collects, layers and joins. Outside a
// region nothing gathers it, so the emitter's fallback would run it as a bare
// `go func() { ... }()` — the write below would race the read that follows it,
// silently, with zero diagnostics. That is the exact class Tier 0 refuses.
//
// ---- Design (issue #1962) ----
//   * `task { ... }` is the auto region's task form; the spawn form for
//     everything else is `task f(...)` (C1), whose sendability check-sites are
//     what the rest of the corpus pins.
//   * The rule is placement, not syntax: the same block one line up, inside
//     `auto { ... }`, is the accepted shape (corpus 130).
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: REJECT AUTO-SCOPE
// EXPECTED-DIAGNOSTIC: HYB-E3206
//   Error: HYB-E3206 — `task { ... }` in a position the scheduler cannot
//   collect (no enclosing `auto` at all here).
// ============================================================================

fn main() {
    x := 0

    task {
        x = 1                 // NO REGION: nobody joins this spawn
    }

    fmt.println("x = {x}")
}
