// EXPECTED-VERDICT: REJECT HYB-E3710
// Rule O4.11a: pool.Put requires explicit `move` consumption at CheckSite::PoolPut.
// Passing an unmoved binding leaves the caller with an alias across pool epochs.

struct packet {
    id int
    payload []int
}

fn main() -> int {
    pool := sync.NewPool[packet]()
    pkt := pool.Get()
    pkt.id = 10

    // Missing `move` keyword at CheckSite::PoolPut:
    pool.Put(pkt)
    return 0
}
