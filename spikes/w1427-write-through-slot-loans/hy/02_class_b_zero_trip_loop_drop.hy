// W-1427 Reproducer 02 — Class B: Zero-Trip Loop Handoff Drops Store-Back Across `move m`
//
// Under Deferred Store-Back at Last Lexical Use (`branch_inherit` / `flushed_before`):
// - `n.bal = 99` executes unconditionally before the `for i in 0..0` loop.
// - Because the `for` body is the last user of `n` and also mentions `m`, `hy-emit` hands off
//   `m["a"] = n` into `Stmt::For` via `branch_inherit` and marks `n` as `flushed_before`.
// - On a 0-trip loop (`0..0` / `for i := 0; i < 0; i++`), the loop body never executes, so
//   `m["a"] = n` is dropped permanently before `task worker(move m, done)` transfers ownership.
//   The spawned worker observes `m["a"].bal == 10` (`worker saw 99? false`).
//
// Under Write-Through Exclusive Loans (#1427):
// - `m["a"] = n` is emitted immediately on the statement `n.bal = 99` before the loop and spawn.
// - `worker` deterministically observes `m["a"].bal == 99` (`worker saw 99? true`).

struct acct { bal int }

fn worker(m map[string]acct, done chan bool) {
    done <- (m["a"].bal == 99)
}

fn main() {
    m := map[string]acct{"a": acct{bal: 10}}
    n := &m["a"]
    n.bal = 99
    done := make(chan bool, 1)
    for i in 0..0 {
        fmt.println(n.bal, i)
        fmt.println(m["a"].bal)
    }
    task worker(move m, done)
    ok := <-done
    fmt.println("worker saw 99?", ok)
}
