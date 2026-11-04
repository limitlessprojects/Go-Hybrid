// ============================================================================
// 141-auto-statement-after-task-rejected — Auto scheduling corpus #141 (issue #1962).
//
// PURPOSE: negative control — the region's statement order. `auto { ... }`
// emits its non-task statements first and then the task layers (that is what
// makes the layer set statically known and joinable). A statement written
// AFTER a task therefore runs BEFORE it: the program says "set x, then print
// x" and the emitted Go prints the old value. Silent, and wrong.
//
// ---- What the emitter would have produced ----
//   { fmt.Println("x = 0-ish"); x = 1 }        // layer 0 is a single task: inline
// not
//   { x = 1; fmt.Println("x = 1") }            // what the source order says
//
// ---- Design (issue #1962) ----
//   * A statement written AHEAD of the region's first task keeps its order —
//     that shape is legal and unchanged.
//   * Only the mix that contradicts itself is refused (HYB-E3207, AUTO-ORDER):
//     the fix is one line of movement, not a different construct.
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: REJECT AUTO-ORDER
// EXPECTED-DIAGNOSTIC: HYB-E3207
//   Error: HYB-E3207 — statement written after a task inside an auto region.
// ============================================================================

fn main() {
    x := 0

    auto {
        task {
            x = 1
        }
        fmt.println("x = {x}")   // WOULD RUN BEFORE THE TASK: must be rejected
    }
}
