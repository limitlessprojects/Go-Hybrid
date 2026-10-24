// EXPECTED-CANCEL: E8002 x1
// `<-jobs` ignores the closed `done`.
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
