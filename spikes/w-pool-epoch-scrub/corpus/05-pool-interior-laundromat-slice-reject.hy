// EXPECTED-VERDICT: REJECT HYB-E3711
// Rule O4.11a (The Interior Laundromat Hazard):
// Planting a surviving exterior slice `shared_slice` into `pkt.payload` and calling
// `pool.Put(move pkt)` while `shared_slice` remains live in the caller is rejected
// because `root_group(pkt).live_bindings != {pkt}`.

struct packet {
    id int
    payload []int
}

fn main() -> int {
    pool := sync.NewPool[packet]()
    shared_slice := []int{1, 2, 3}

    pkt := pool.Get()
    pkt.payload = shared_slice
    pool.Put(move pkt)

    // Exterior alias survives across Put epoch boundary:
    return shared_slice[0]
}
