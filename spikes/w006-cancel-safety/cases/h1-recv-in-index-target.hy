// EXPECTED-CANCEL: E8002 x1
// Receive inside an assignment index target.
fn worker(done chan bool, jobs chan int) {
    m := map[int]int{}
    m[<-jobs] = 1
    fmt.println("{len(m)}")
    <-done
}
fn main() {
    done := make(chan bool)
    jobs := make(chan int)
    task worker(done, jobs)
    close(done)
}
