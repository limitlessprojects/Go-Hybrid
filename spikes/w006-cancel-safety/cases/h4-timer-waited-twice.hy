// EXPECTED-CANCEL: E8002 x2
// time.after waited on twice: the second select can block forever (the first is a conservative FP).
fn worker(done chan bool, jobs chan int) {
    t := time.after(time.seconds(1))
    select {
        v := <-jobs: { fmt.println("{v}") }
        <-t: { fmt.println("first") }
    }
    select {
        v := <-jobs: { fmt.println("{v}") }
        <-t: { fmt.println("second") }
    }
    <-done
}
fn main() {
    done := make(chan bool)
    jobs := make(chan int)
    task worker(done, jobs)
    close(done)
}
