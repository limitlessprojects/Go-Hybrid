// EXPECTED-CANCEL: E8002 x1
// Reassigning the token param drops it.
fn worker(done chan bool) {
    done = make(chan bool)
    <-done
}

fn main() {
    done := make(chan bool)
    task worker(done)
    close(done)
}
