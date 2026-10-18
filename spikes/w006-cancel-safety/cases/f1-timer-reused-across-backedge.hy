// EXPECTED-CANCEL: E8002 x1
// time.after fires once: bound outside the loop, it is drained after the first timeout.
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
