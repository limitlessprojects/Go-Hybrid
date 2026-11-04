// ============================================================================
// 137-auto-blocking-select-rejected — Auto scheduling corpus #137 (issue #1962).
//
// PURPOSE: negative control — a `select` inside an auto region must be
// rejected even though the region itself is well shaped (two direct inline
// task blocks). The scheduler collects the task set and joins each layer; a
// clause that waits on channels owned by nobody in the region can park a task
// forever, so the join never happens.
//
// ---- Design (issue #1962: "closed scope") ----
//   * `auto { ... }` is fork-join: every task must terminate for the region
//     to reach its `wg.Wait()`.
//   * A `select` is not a data dependency the effect model can see — it is a
//     blocking wait, so the region refuses it (HYB-E3202, AUTO-SELECT).
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: REJECT AUTO-SELECT
// EXPECTED-DIAGNOSTIC: HYB-E3202
//   Error: HYB-E3202 — select statement inside auto scope.
//
// The receive sits INSIDE the select clause on purpose: the clause as a whole
// is the rejected construct, so exactly one code is registered here (the
// checker does not descend into select arms for a second receive error).
// ============================================================================

fn main() {
    a := make(chan int, 1)
    total := 0

    auto {
        task {
            a <- 1
        }
        task {
            select {                      // BLOCKING WAIT: must be rejected
                v := <-a:
                    total += v
            }
        }
    }

    fmt.println(total)
}
