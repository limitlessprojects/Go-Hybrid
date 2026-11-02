// 02-pipeline-select-timeout-rejected.hy
// examples/04-pipeline-backpressure as shipped: the producer -> squarer
// pipeline is SPSC, but the `select` timeout arm (`<-deadline`) introduces
// schedule-dependent readiness arbitration (HYB-E3503) and wall-clock coupling
// (HYB-E3504). Rejected from the Kahn determinism class.

fn producer(out chan<- int) {
    for i in 1..=8 {
        out <- i
    }
    close(out)
}

fn squarer(in <-chan int, out chan<- int) {
    for v in in {
        time.sleep(time.milliseconds(5))
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
    deadline := time.after(time.seconds(2))
    loop {
        select {
            v, ok := <-sq:
                if !ok { break loop }
                total += v
            <-deadline:
                panic "pipeline timed out"
        }
    }
    fmt.println("total={total}")
}
