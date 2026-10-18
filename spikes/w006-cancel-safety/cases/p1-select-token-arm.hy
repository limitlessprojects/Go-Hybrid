// EXPECTED-CANCEL: CLEAN
// The worker's only blocking site is a select with a receive arm on `done`,
// which main closes: done is a token, the select is cancel-observable.
fn worker(done chan bool, jobs chan int) {
    loop {
        select {
            v := <-jobs: fmt.println("{v}")
            <-done: return
        }
    }
}

fn main() {
    done := make(chan bool)
    jobs := make(chan int)
    task worker(done, jobs)
    jobs <- 1
    close(done)
}
