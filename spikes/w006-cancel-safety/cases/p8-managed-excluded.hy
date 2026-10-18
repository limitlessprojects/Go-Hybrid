// EXPECTED-CANCEL: CLEAN
// A bare receive inside managed is outside the claim (counted by
// --report-managed, never by the cancel pass).
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
