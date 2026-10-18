// EXPECTED-CANCEL: E8002 x1
// Method task blocks on a field channel. (Tier-0 also rejects this; defense in depth.)
struct pump {
    jobs chan int
}

fn (p pump) run(done chan bool) {
    v := <-p.jobs
    fmt.println("{v}")
    <-done
}

fn main() {
    done := make(chan bool)
    p := pump{jobs: make(chan int)}
    task p.run(done)
    close(done)
}
