// EXPECTED-CANCEL: CLEAN
// A select with default never blocks.
fn worker(jobs chan int) {
    select {
        v := <-jobs: fmt.println("{v}")
        default: fmt.println("idle")
    }
}

fn main() {
    jobs := make(chan int)
    task worker(jobs)
}
