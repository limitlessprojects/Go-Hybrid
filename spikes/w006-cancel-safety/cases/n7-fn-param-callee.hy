// EXPECTED-CANCEL: E8002 x1
// Closure returned by a call: body not visible, fail closed.
fn make_step() fn() {
    return fn() { fmt.println("tick") }
}

fn worker(done chan bool) {
    step := make_step()
    step()
    <-done
}

fn main() {
    done := make(chan bool)
    task worker(done)
    close(done)
}
