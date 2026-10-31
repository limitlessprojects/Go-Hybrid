// EXPECTED-CANCEL: CLEAN
// Sites inside `managed` are outside the claim.
fn worker(jobs chan int) {
    managed("drained by an external protocol", "other") {
        v := <-jobs
        fmt.println("{v}")
    }
}

fn main() {
    jobs := make(chan int)
    task worker(jobs)
}
