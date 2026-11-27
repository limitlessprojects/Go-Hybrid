// W-4843 Reproducer 01 — corpus #19's zero-copy twin (SPEC-LANGUAGE.md §13.2 D1).
//
// Plain `move` (corpus #19) still bans a post-send sender read — `sink`
// receives a mutable map and may execute `m["fox"]++`, so the sender reading
// `agg` after `move agg` is sent would race with it:
//
//   results <- move agg
//   n := len(agg)          // REJECT HYB-E3003: `agg` was moved
//
// `seal move` is the missing escape: it freezes `agg`'s entire storage
// component (no writer can exist anywhere in it, by rule O5.7) and returns a
// Copy, read-only witness that both the sender and every receiver may read.

fn main() {
    agg := map[string]int{"fox": 1, "hare": 2}

    results := make(chan sealed[map[string]int], 4)
    snap := seal move agg

    for i := 0; i < 4; i++ {
        results <- snap
    }

    n := snap.len()          // ACCEPT: reading the witness, not the moved-from binding.
    fmt.println("sender n=", n)

    for i := 0; i < 4; i++ {
        got := <-results
        fmt.println("receiver len=", got.len())
    }
}
