// EXPECTED-CANCEL: E8002 x1
// A task spawned inside a foreign callback is still checked code.
fn worker(done chan bool, jobs chan int) {
    v := <-jobs
    fmt.println("{v}")
    <-done
}

fn main() {
    done := make(chan bool)
    jobs := make(chan int)
    sort.slice([]int{2, 1}, fn(i int, j int) bool {
        task worker(done, jobs)
        return i < j
    })
    close(done)
}
