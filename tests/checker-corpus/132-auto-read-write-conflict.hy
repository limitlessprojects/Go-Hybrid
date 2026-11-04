// ============================================================================
// 132-auto-read-write-conflict — Auto scheduling corpus #132 (issue #1962).
//
// PURPOSE: read-write conflict detection inside `auto { ... }`.
// Task 0 writes `total`, task 1 reads `total`. The scheduler must detect
// the write-read dependency and serialize them.
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: ACCEPT
//   The tasks conflict (write-read on `total`), so they are serialized.
//   Task 0 runs first, then task 1. No data race.
// ============================================================================

fn main() {
    total := 0

    auto {
        task {
            total = 100
        }
        task {
            doubled := total * 2
            fmt.println("doubled = {doubled}")
        }
    }
}
