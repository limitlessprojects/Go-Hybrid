// EXPECTED-CANCEL: CLEAN
// Fresh `time.after` arm bounds the wait.
fn worker(jobs chan int) {
    deadline := time.after(time.seconds(1))
    select {
        v := <-jobs: fmt.println("{v}")
        <-deadline: fmt.println("timeout")
    }
}

fn main() {
    jobs := make(chan int)
    task worker(jobs)
}
