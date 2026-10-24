// EXPECTED-CANCEL: E8002 x1
// Result send: close() never unblocks a sender.
fn worker(jobs chan int, results chan int) {
    for j in jobs {
        results <- j * 2
    }
}

fn main() {
    jobs := make(chan int, 4)
    results := make(chan int)
    task worker(jobs, results)
    jobs <- 1
    close(jobs)
}
