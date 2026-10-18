// EXPECTED-CANCEL: CLEAN
// `for j in jobs` is a receive loop; main closes `jobs`, so jobs is a token
// and the loop terminates once it is closed (example 03's worker shape,
// minus the result send).
fn worker(jobs chan int) {
    for j in jobs {
        fmt.println("{j}")
    }
}

fn main() {
    jobs := make(chan int, 4)
    task worker(jobs)
    jobs <- 1
    close(jobs)
}
