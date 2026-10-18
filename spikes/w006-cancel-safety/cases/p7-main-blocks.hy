// EXPECTED-CANCEL: CLEAN
// main (root) is not claimed.
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
