// 04-pipeline-backpressure — stresses: bounded channels as backpressure, select
// with timeout, closing pipelines in order. TARGET SYNTAX.

fn producer(out chan<- int) {
    for i in 1..=8 {
        out <- i                          // blocks when stage 2 is slow: backpressure
    }
    close(out)
}

fn squarer(in <-chan int, out chan<- int) {
    for v in in {
        time.sleep(time.milliseconds(5)) // deliberately the slow stage
        out <- v * v
    }
    close(out)
}

fn main() {
    raw := make(chan int, 2)              // SMALL buffer: backpressure is observable
    sq := make(chan int, 2)

    task producer(raw)
    task squarer(raw, sq)

    var total int
    deadline := time.after(time.seconds(2))
    loop {
        select {
            v, ok := <-sq:
                if !ok { break loop }     // pipeline drained
                total += v
            <-deadline:
                panic "pipeline timed out" // never fires in the happy path
        }
    }
    fmt.println("total={total}")          // 1+4+9+...+64 = 204
}
