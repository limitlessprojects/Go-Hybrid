// EXPECTED-CANCEL: E8002 x1
// FALSIFIER (hunt round 1, found against pass v1): time.after fires ONCE.
// `deadline` is bound outside the loop, so after the first timeout the
// select's timer arm is drained and the next iteration can block forever on
// `jobs` — the loop back-edge turns a bounded arm into an unbounded one.
fn worker(done chan bool, jobs chan int) {
    deadline := time.after(time.seconds(1))
    loop {
        select {
            v := <-jobs: { fmt.println("{v}") }
            <-deadline: { fmt.println("tick") }
        }
    }
    <-done
}

fn main() {
    done := make(chan bool)
    jobs := make(chan int)
    task worker(done, jobs)
    close(done)
}
