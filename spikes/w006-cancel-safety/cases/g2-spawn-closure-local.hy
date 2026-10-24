// EXPECTED-CANCEL: E8002 x1
// Spawned closure ignores `done`. (Tier-0 also rejects this; defense in depth.)
fn main() {
    done := make(chan bool)
    jobs := make(chan int)
    run := fn() {
        v := <-jobs
        fmt.println("{v}")
        <-done
    }
    task run()
    close(done)
}
