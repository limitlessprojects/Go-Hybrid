// EXPECTED-CANCEL: E8002 x1
// main closes a shadowing `done`, not the worker's.
fn worker(done chan bool) {
    <-done
}

fn main() {
    done := make(chan bool)
    task worker(done)
    {
        done := make(chan bool)
        close(done)
    }
}
