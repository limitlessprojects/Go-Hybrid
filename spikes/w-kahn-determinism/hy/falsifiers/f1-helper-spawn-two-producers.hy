// f1-helper-spawn-two-producers.hy — H1 falsifier for #2475.
// Four tasks send on one channel, but each `task` sits in its own call to
// `spawn_sender`. The classifier counts producers per function body, so it
// sees one producer in `spawn_sender` and none in `main`, and admits the
// program. The channel has four producers, so arrival order varies.
// Correct verdict: REJECT (HYB-E3501). Needs interprocedural endpoint tracking.

fn sender(id int, out chan<- int) {
    out <- id
}

fn spawn_sender(id int, out chan<- int) {
    task sender(id, out)
}

fn main() {
    out := make(chan int, 4)
    spawn_sender(1, out)
    spawn_sender(2, out)
    spawn_sender(3, out)
    spawn_sender(4, out)
    a := <-out
    b := <-out
    c := <-out
    d := <-out
    fmt.println("order={a}{b}{c}{d}")
}
