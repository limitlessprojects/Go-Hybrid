// EXPECTED-CANCEL: E8002 x1
// main's close() targets a SHADOWING `done`, not the channel the worker
// holds: the worker's done is not a token.
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
