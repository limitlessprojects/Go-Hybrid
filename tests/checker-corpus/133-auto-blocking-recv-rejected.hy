// ============================================================================
// 133-auto-blocking-recv-rejected — Auto scheduling corpus #133 (issue #1962).
//
// PURPOSE: negative control — blocking channel receive inside an auto block
// must be rejected. The scheduler needs all tasks upfront; a blocking receive
// would deadlock the parallel layer.
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: REJECT AUTO-RECV
// EXPECTED-DIAGNOSTIC: HYB-E3201
//   Error: HYB-E3201 — channel receive inside auto scope.
// ============================================================================

fn main() {
    ch := make(chan int, 1)
    result := 0

    auto {
        task {
            ch <- 42
        }
        task {
            result = <-ch  // BLOCKING: must be rejected
        }
    }

    fmt.println("result = {result}")
}
