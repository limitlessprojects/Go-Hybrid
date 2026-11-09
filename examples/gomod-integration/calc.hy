// calc.hy — the Hybrid-checked file: ownership-checked concurrent sum.
// Transpiled to calc.hy.go by `hyb generate` (run via go:generate).
// Delete this file and calc.hy.go remains as plain, reviewable Go.
//
// `pub` makes SumConcurrent visible to app.go (LX5). The checker
// verifies every cross-task value here: shard sums travel over
// a channel as primitive copies; tasks own per-task state; nothing shares
// a mutable slice view across a boundary (a slice VIEW would reject with
// HYB-E3001 — the fix-its name the legal routes).

fn sum_shard(jobs <-chan int, out chan<- int) {
    acc := 0
    for x in jobs {
        acc += x
    }
    out <- acc
}

pub fn sum_concurrent(xs []int, shards int) int {
    if shards < 1 {
        return 0
    }
    jobs := make(chan int, 16)
    out := make(chan int, shards)
    for i in 0..shards {
        task sum_shard(jobs, out)
    }
    for x in xs {
        jobs <- x
    }
    close(jobs)
    total := 0
    for _ in 0..shards {
        total += <-out
    }
    return total
}
