// EXPECTED-CANCEL: E8002 x1
// A task-local close is not a token: the helper's wait self-deadlocks.
fn helper(c chan bool) {
    <-c
}

fn worker(done chan bool) {
    c := make(chan bool)
    helper(c)
    close(c)
    <-done
}

fn main() {
    done := make(chan bool)
    task worker(done)
    close(done)
}
