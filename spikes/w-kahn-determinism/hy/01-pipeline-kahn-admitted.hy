// 01-pipeline-kahn-admitted.hy
// Derived from examples/04-pipeline-backpressure by removing the `select`
// timeout arm (`<-deadline`) and `time.sleep`.
// Strictly 1 producer and 1 consumer per channel -> admitted into the
// Kahn schedule-independent determinism class (0 diagnostics).

fn producer(out chan<- int) {
    for i in 1..=8 {
        out <- i
    }
    close(out)
}

fn squarer(in <-chan int, out chan<- int) {
    for v in in {
        out <- v * v
    }
    close(out)
}

fn main() {
    raw := make(chan int, 2)
    sq := make(chan int, 2)

    task producer(raw)
    task squarer(raw, sq)

    var total int
    for v in sq {
        total += v
    }
    fmt.println("total={total}")          // 1+4+9+...+64 = 204 on every schedule
}
