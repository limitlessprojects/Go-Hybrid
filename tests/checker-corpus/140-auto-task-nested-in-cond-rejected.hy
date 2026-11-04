// ============================================================================
// 140-auto-task-nested-in-cond-rejected — Auto scheduling corpus #140 (issue #1962).
//
// PURPOSE: negative control — `task { ... }` inside an auto region but NOT as
// a direct statement of its body: here it is wrapped in an `if`. The region
// collects its task set from the body's direct statements (that is what
// `hy-emit`'s scheduler walks), so a conditional task never enters a layer.
// It leaves the region through the goroutine fallback and races the code after
// the block — the same hole as corpus 139, reached from inside the region.
//
// ---- Design (issue #1962) ----
//   * Closed scope, literally: the task set is the set of direct inline
//     blocks, known at compile time (HYB-E3206, AUTO-SCOPE).
//   * Branching belongs outside the region — two `auto` blocks, or a plain
//     `task f(...)` spawn with the program's own join (example 03).
//   * A task in a LOOP inside the region is the neighbouring rule and keeps
//     its own code (HYB-E3203, dynamic spawn — corpus 134); a task under a
//     condition has no loop to blame, so AUTO-SCOPE is the only thing to say.
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: REJECT AUTO-SCOPE
// EXPECTED-DIAGNOSTIC: HYB-E3206
//   Error: HYB-E3206 — `task { ... }` is not a direct statement of the body.
// ============================================================================

fn main() {
    ready := true
    x := 0

    auto {
        if ready {
            task {                 // NOT A DIRECT STATEMENT: never scheduled
                x = 1
            }
        }
        task {
            x = x + 1
        }
    }

    fmt.println("x = {x}")
}
