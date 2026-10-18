// EXPECTED-CANCEL: CLEAN
// `for … in` over a token terminates on close.
fn worker(jobs chan int) {
    for j in jobs {
        fmt.println("{j}")
    }
}

fn main() {
    jobs := make(chan int, 4)
    task worker(jobs)
    jobs <- 1
    close(jobs)
}
