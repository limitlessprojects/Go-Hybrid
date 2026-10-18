// EXPECTED-CANCEL: E8002 x1
// FALSIFIER (hunt round 1, found against pass v1): the spawn sits inside a
// managed block, so v1 never discovered it — but the spawned task's BODY is
// ordinary checked code and blocks on a non-token after `done` is closed.
fn worker(done chan bool, jobs chan int) {
    v := <-jobs
    fmt.println("{v}")
    <-done
}

fn main() {
    done := make(chan bool)
    jobs := make(chan int)
    managed("spawn audited by hand", "other") {
        task worker(done, jobs)
    }
    close(done)
}
