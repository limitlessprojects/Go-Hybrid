// EXPECTED-CANCEL: CLEAN
// `select` with a token arm.
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
