// EXPECTED-VERDICT: REJECT HYB-E3711
// Falsifier F1 (Sub-Slice Loan Escape):
// Deriving `sub := pkt.payload[0:2]` before `pool.Put(move pkt)` adds `sub` to `pkt`'s
// Polonius root group. Retaining capacity via `s[:0:cap(s)]` would let the next epoch's
// `append` overwrite `sub[0]`, so O4.11a rejects with HYB-E3711 at `pool.Put(move pkt)`.

struct packet {
    id int
    payload []int
}

fn main() -> int {
    pool := sync.NewPool[packet]()
    pkt := pool.Get()
    pkt.payload = []int{10, 20, 30}

    sub := pkt.payload[0:2]
    pool.Put(move pkt)

    return sub[0]
}
