// 03-worker-pool-commutative-sink.hy
// examples/03-worker-pool as shipped: four workers share `jobs` (HYB-E3502
// multi-consumer) and `results` (HYB-E3501 multi-producer), so channel
// arrival order varies across schedules (653 distinct permutations observed in
// 1,000 runs). Rejected by pure SPSC Kahn, yet output is 100% deterministic
// because `sort.ints(got)` is a commutative sink (canonical false negative).

struct job {
    input int
}

struct result {
    worker int
    value  int
}

fn worker(id int, jobs <-chan job, results chan<- result) {
    for j in jobs {
        results <- result{worker: id, value: j.input * 2}
    }
}

fn main() {
    const n_workers = 4
    jobs := make(chan job, 16)
    results := make(chan result, 16)

    for i in 0..n_workers {
        task worker(i, jobs, results)
    }

    inputs := [3, 1, 4, 1, 5, 9, 2, 6]
    for _, v in inputs {
        jobs <- job{input: v}
    }
    close(jobs)

    var got []int
    for len(got) < len(inputs) {
        r := <-results
        got = append(got, r.value)
    }

    sort.ints(got)
    fmt.println(got)
}
