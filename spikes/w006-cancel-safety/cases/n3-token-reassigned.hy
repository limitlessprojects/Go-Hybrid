// EXPECTED-CANCEL: E8002 x1
// The worker rebinds its token param to a fresh channel, so receiving from
// `done` no longer observes the caller's close.
fn worker(done chan bool) {
    done = make(chan bool)
    <-done
}

fn main() {
    done := make(chan bool)
    task worker(done)
    close(done)
}
