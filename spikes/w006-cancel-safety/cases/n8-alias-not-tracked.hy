// EXPECTED-CANCEL: E8002 x1
// Known false positive: aliases (`d := done`) are not tracked.
fn worker(done chan bool) {
    <-done
}

fn main() {
    done := make(chan bool)
    d := done
    task worker(d)
    close(done)
}
