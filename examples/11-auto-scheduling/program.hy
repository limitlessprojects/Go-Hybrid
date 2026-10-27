// 11-auto-scheduling — automatic fork-join scheduling (issue #1962).
//
// TARGET: demonstrate the `auto { ... }` scope:
//   * Independent tasks (disjoint variables) run in parallel
//   * Conflicting tasks (shared variables) are serialized
//   * No manual ownership annotations required
//
// Expected output is deterministic (sorted at the end).

fn main() {
    a := 0
    b := 0
    counter := 0

    // Layer 0: these two tasks write disjoint variables (a and b),
    // so the scheduler runs them in parallel.
    // Layer 1: this task writes `counter`, which conflicts with...
    // Layer 2: this task also writes `counter`, so they are serialized.
    auto {
        task {
            a = 10
        }
        task {
            b = 20
        }
        task {
            counter = counter + 1
        }
        task {
            counter = counter + 1
        }
    }

    fmt.println("a = {a}, b = {b}")
    fmt.println("counter = {counter}")
}
