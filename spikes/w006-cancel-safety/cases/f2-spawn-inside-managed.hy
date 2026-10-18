// EXPECTED-CANCEL: E8002 x1
// A task spawned inside `managed` is still checked code.
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
