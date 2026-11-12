// EXPECTED-VERDICT: ACCEPT
// Concurrent task handoff of pool-acquired struct followed by clean recycling inside worker.

struct packet {
    id int
    payload []int
}

fn recycle_worker(pkt packet, pool sync.Pool[packet], done chan int) {
    pkt.id = pkt.id + 10
    pool.Put(move pkt)
    done <- 1
}

fn main() -> int {
    pool := sync.NewPool[packet]()
    done := make(chan int, 1)

    p := pool.Get()
    p.id = 5
    task recycle_worker(move p, pool, done)

    <-done
    return 0
}
