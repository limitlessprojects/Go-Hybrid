// EXPECTED-CANCEL: E8002 x1
// Documented conservative false positive: `d` aliases `done`, but the pass
// does not track aliases, so the worker's receive is not proven observable.
fn worker(done chan bool) {
    <-done
}

fn main() {
    done := make(chan bool)
    d := done
    task worker(d)
    close(done)
}
