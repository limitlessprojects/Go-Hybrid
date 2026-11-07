// EXPECTED-VERDICT: REJECT HYB-E3003
// Rule O4.11a + O5.2: pool.Put(move pkt) consumes `pkt`; any post-Put read or write is rejected.

struct packet {
    id int
    payload []int
}

fn main() -> int {
    pool := sync.NewPool[packet]()
    pkt := pool.Get()
    pkt.id = 10
    pool.Put(move pkt)

    // Illegal use-after-move after pool.Put(move pkt):
    return pkt.id
}
