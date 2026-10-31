// EXPECTED-CANCEL: CLEAN
// Token passes through a nested spawn.
fn child(done chan bool) {
    <-done
}

fn parent(done chan bool) {
    task child(done)
}

fn main() {
    done := make(chan bool)
    task parent(done)
    close(done)
}
