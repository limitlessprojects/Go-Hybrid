// ============================================================================
// 131-auto-serialize-conflict — Auto scheduling corpus #131 (issue #1962).
//
// PURPOSE: negative control for conflicting tasks inside `auto { ... }`.
// Two inline task blocks BOTH write the same variable `counter`. The
// scheduler must detect the write-write conflict and serialize them
// (run one after the other) to avoid a data race.
//
// ---- Design (issue #1962) ----
//   * Default = serial: if independence cannot be proven, run in order
//   * Parallel only when data accesses are provably disjoint
//   * No race, no rejection — just slower
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: ACCEPT
//   The two tasks conflict (both write `counter`), so they are serialized.
//   No data race. No compile error — that's the whole point of `auto`.
//
// ---- Status ----
//   MVP: effect analysis detects write-write overlap.
//   Serialized emission is in progress.
// ============================================================================

fn main() {
    counter := 0

    auto {
        task {
            counter = counter + 1
        }
        task {
            counter = counter + 1
        }
    }

    fmt.println("counter = {counter}")
}
