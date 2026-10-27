// ============================================================================
// 134-auto-dynamic-spawn-rejected — Auto scheduling corpus #134 (issue #1962).
//
// PURPOSE: negative control — task block inside a loop (dynamic spawn) must
// be rejected. The task set must be statically known at compile time.
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: REJECT
//   Error: HYB-E3013 — task block inside a loop.
// ============================================================================

fn main() {
    results := make(chan int, 4)

    auto {
        for i in 0..4 {
            task {          // DYNAMIC SPAWN: must be rejected
                results <- i * 2
            }
        }
    }

    close(results)
}
