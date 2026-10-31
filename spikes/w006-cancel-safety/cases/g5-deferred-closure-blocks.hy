// EXPECTED-CANCEL: E8002 x1
// Blocking receive inside a deferred closure.
fn worker(done chan bool, jobs chan int) {
    defer fn() {
        v := <-jobs
        fmt.println("{v}")
    }()
    <-done
}

fn main() {
    done := make(chan bool)
    jobs := make(chan int)
    task worker(done, jobs)
    close(done)
}
