// EXPECTED-CANCEL: E8002 x1
// main closes `done` (a token) but the worker blocks on `<-jobs`, which
// nobody sends to or closes: blocks forever after cancellation.
fn worker(done chan bool, jobs chan int) {
    v := <-jobs
    fmt.println("{v}")
    <-done
}

fn main() {
    done := make(chan bool)
    jobs := make(chan int)
    task worker(done, jobs)
    close(done)
}
