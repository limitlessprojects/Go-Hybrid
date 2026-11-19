// 03-worker-pool — stresses: tasks (goroutines), channels, select, WaitGroup-like join.
// TARGET SYNTAX. Fixed input => deterministic output (sorted at the end).

struct job {
    input   int
    cost_ms int
}

struct result {
    worker int
    value  int
}

fn worker(id int, jobs <-chan job, results chan<- result) {
    for j in jobs {                       // ranges until channel close
        // simulate uneven work
        time.sleep(time.milliseconds(j.cost_ms))
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
        jobs <- job{input: v, cost_ms: (v % 3)}
    }
    close(jobs)                           // no more work; workers drain and exit

    var got []int
    for len(got) < len(inputs) {
        r := <-results                    // receive until all answers arrived
        got = append(got, r.value)
    }

    sort.ints(got)
    fmt.println(got)
}
