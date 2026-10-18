// EXPECTED-CANCEL: E8002 x1
// The task invokes a closure obtained from a call: its body is not visible
// at the call site, so the pass fails closed.
// (Amended before the pass existed: a fn-typed spawn ARGUMENT is rejected by
// Tier-0 itself — HYB-E3005 — so the unresolved callee is built in-task.)
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
