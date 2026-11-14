// ============================================================================
// hy-pipe-capabilities-accept — SPSC capability pipe emit path (#4464).
//
// PURPOSE: exercises the checker-accepted SPSC capability surface through full
//   code generation and execution: hy.pipe[T](), hy.new_pipe[T](cap),
//   hy.send_cap[T], hy.recv_cap[T], .send(), .close(), .recv(), .drain(),
//   and single-var `for v in rx` iteration.
//
// EXPECTED-VERDICT: ACCEPT
// EXPECTED-OUTPUT: 3 9
// ============================================================================

import "fmt"

fn worker(sender hy.send_cap[int]) {
    for i in 0..3 {
        sender.send(i)
    }
    sender.close()
}

fn forwarder(upstream hy.recv_cap[int], downstream hy.send_cap[int]) {
    first := upstream.recv()
    downstream.send(first)
    upstream.drain()
    downstream.close()
}

fn main() {
    tx1, rx1 := hy.pipe[int]()
    task worker(move tx1)
    total := 0
    for v in rx1 {
        total = total + v
    }

    tx2, rx2 := hy.new_pipe[int](4)
    tx3, rx3 := hy.pipe[int]()
    tx2.send(9)
    tx2.close()
    task forwarder(move rx2, move tx3)
    tail := rx3.recv()
    rx3.drain()
    fmt.println(total, tail)
}
