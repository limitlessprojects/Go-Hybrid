// EXPECTED-CANCEL: E8002 x1
// The helper blocks on `jobs`; only `done` is a token in the worker.
fn take(jobs chan int) int {
    return <-jobs
}

fn worker(done chan bool, jobs chan int) {
    v := take(jobs)
    fmt.println("{v}")
    <-done
}

fn main() {
    done := make(chan bool)
    jobs := make(chan int)
    task worker(done, jobs)
    close(done)
}
