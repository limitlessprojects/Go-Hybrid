// EXPECTED-CANCEL: CLEAN
// main is the root task: nothing cancels it, so its blocking sites are
// outside the claim (root mode reports nothing).
fn worker(results chan int) {
    select {
        results <- 1: {}
        default: {}
    }
}

fn main() {
    results := make(chan int, 1)
    task worker(results)
    v := <-results
    fmt.println("{v}")
}
