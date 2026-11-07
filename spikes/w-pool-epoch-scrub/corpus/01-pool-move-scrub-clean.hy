// EXPECTED-VERDICT: ACCEPT
// Flip of corpus 18-pool-shared-struct-managed.hy:
// Zero `managed` blocks and zero capability annotations under Rule O4.11.

struct packet {
    id int
    payload []int
}

fn worker(pkt packet, out chan int) {
    out <- pkt.id
}

fn main() -> int {
    pool := sync.NewPool[packet]()
    ch := make(chan int, 1)

    pkt := pool.Get()
    pkt.id = 42
    pkt.payload = []int{1, 2, 3}

    // Return to pool via O4.11 consuming move-scrub check-site:
    pool.Put(move pkt)

    // Re-acquire fresh epoch-isolated instance and hand off to task:
    pkt2 := pool.Get()
    pkt2.id = 7
    task worker(move pkt2, ch)

    v := <-ch
    return v - 7
}
