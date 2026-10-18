// EXPECTED-CANCEL: E8002 x2
// Named args: positions must not be used to map tokens (fail closed: both receives flagged).
fn worker(jobs chan int, done chan bool) {
    <-jobs
    <-done
}
fn main() {
    done := make(chan bool)
    jobs := make(chan int)
    task worker(done=done, jobs=jobs)
    close(done)
}
