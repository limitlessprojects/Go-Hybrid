// EXPECTED-CANCEL: E8002 x2
// Closure via fn-typed field: flagged at its definition and fail-closed at `h.step()`.
struct hooks {
    step fn()
}

fn make_hooks(jobs chan int) hooks {
    return hooks{step: fn() { <-jobs }}
}

fn worker(done chan bool, jobs chan int) {
    h := make_hooks(jobs)
    h.step()
    <-done
}

fn main() {
    done := make(chan bool)
    jobs := make(chan int)
    task worker(done, jobs)
    close(done)
}
