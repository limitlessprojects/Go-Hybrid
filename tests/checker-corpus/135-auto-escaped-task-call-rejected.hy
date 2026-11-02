// ============================================================================
// 135-auto-escaped-task-call-rejected — Auto scheduling corpus #135 (issue #1962).
//
// PURPOSE: negative control — plain `task fn(...)` call inside an auto block
// must be rejected. We can't analyze the callee's effects, so it might escape
// mutable state to the outer goroutine. Only inline `task { ... }` blocks are
// allowed inside auto scope.
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: REJECT AUTO-TASK
// EXPECTED-DIAGNOSTIC: HYB-E3204
//   Error: HYB-E3204 — plain task call inside auto scope.
// ============================================================================

fn helper(x int) int {
    return x * 2
}

fn main() {
    result := 0

    auto {
        task {
            result = 100  // inline block: OK
        }
        task helper(result)  // plain call: REJECTED (can't analyze effects)
    }

    fmt.println("result = {result}")
}
