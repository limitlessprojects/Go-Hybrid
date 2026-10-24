// EXPECTED-CANCEL: E8002 x1
// Closure invoked in the task blocks on a non-token.
fn worker(done chan bool, jobs chan int) {
    take := fn() int { return <-jobs }
    v := take()
    fmt.println("{v}")
    <-done
}

fn main() {
    done := make(chan bool)
    jobs := make(chan int)
    task worker(done, jobs)
    close(done)
}
