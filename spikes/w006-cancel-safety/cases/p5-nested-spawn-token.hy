// EXPECTED-CANCEL: CLEAN
// `done` is a token of parent (main closes it); parent re-spawns child with
// it, so it is a token of child too, and child's receive is observable.
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
