// EXPECTED-CANCEL: E8002 x1
// FALSIFIER (hunt round 1, found against pass v1): a closure handed to a
// foreign API is not walked (its own sites run on foreign goroutines), so a
// checked `task` spawned inside it was never discovered.
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
