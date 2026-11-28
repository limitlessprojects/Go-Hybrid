// f3-main-is-second-producer.hy — H1 falsifier for #2475.
// Only `task` arguments are counted as endpoints. `main` sending on the same
// channel it handed to a task is a second producer the classifier never sees.
// Correct verdict: REJECT (HYB-E3501).

fn sender(out chan<- int) {
    out <- 1
}

fn main() {
    out := make(chan int, 2)
    task sender(out)
    out <- 2
    a := <-out
    b := <-out
    fmt.println("order={a}{b}")
}
