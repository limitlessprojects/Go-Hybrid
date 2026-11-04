// ============================================================================
// 130-auto-basic-disjoint — Auto scheduling corpus #130 (issue #1962).
//
// PURPOSE: positive control for the `auto { ... }` automatic fork-join scope.
// Two inline task blocks access DISJOINT variables: task 0 writes `x`,
// task 1 writes `y`. The scheduler should prove they are independent and
// run them in parallel.
//
// ---- Design (issue #1962: "Serial by default: safe execution first") ----
//   * `auto { ... }` — opt-in closed fork-join scope
//   * Tasks with provably disjoint data accesses run in parallel
//   * Conflicting or ambiguous tasks are serialized
//   * Unknown = slower, not unsafe
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: ACCEPT
//   The two tasks write disjoint variables (x vs y), so they are provably
//   independent and may run in parallel. No data race.
//
// ---- Status ----
//   MVP: syntax parsed, effects analyzed, scheduling planned.
//   Full DAG scheduling + WaitGroup emission is in progress.
// ============================================================================

fn main() {
    x := 0
    y := 0

    auto {
        task {
            x = 42
        }
        task {
            y = 100
        }
    }

    fmt.println("x = {x}, y = {y}")
}
