// EXPECTED-CANCEL: CLEAN
// Token passes through a call.
fn wait_or_take(done chan bool, jobs chan int) {
    select {
        v := <-jobs: fmt.println("{v}")
        <-done: fmt.println("cancelled")
    }
}

fn worker(done chan bool, jobs chan int) {
    wait_or_take(done, jobs)
}

fn main() {
    done := make(chan bool)
    jobs := make(chan int)
    task worker(done, jobs)
    close(done)
}
