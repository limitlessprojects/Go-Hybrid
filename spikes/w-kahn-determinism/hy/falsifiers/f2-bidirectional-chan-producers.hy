// f2-bidirectional-chan-producers.hy — H1 falsifier for #2475.
// Producer counting only looks for `chan<-` / `<-chan` in parameter types.
// A plain `chan int` parameter (accepted by hy-check, see corpus #62/#63)
// is never counted, so two producers on `out` are invisible.
// Correct verdict: REJECT (HYB-E3501).

fn sender(id int, out chan int) {
    out <- id
}

fn main() {
    out := make(chan int, 4)
    task sender(1, out)
    task sender(2, out)
    task sender(3, out)
    task sender(4, out)
    a := <-out
    b := <-out
    c := <-out
    d := <-out
    fmt.println("order={a}{b}{c}{d}")
}
