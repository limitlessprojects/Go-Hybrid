// 01-row8-managed-mutex-vs-merge-capsule.hy (#2385)
// BEFORE: Row 8 requires `managed(...)` + `sync.Mutex` for parallel fan-out aggregation.
// AFTER: Tier-0 `merge[Histogram]` capsule splits into wait-free task-local shards.
struct Histogram { buckets [8]int; tags map[string]int }
fn worker(mut s shard[Histogram], id int) {
    for i in 0..1000 { s.buckets[(id + i) & 7] += 1 }
    s.finish()
}
fn main() {
    mut cap := merge[Histogram]{}
    for w in 0..8 {
        sh := cap.split()
        task worker(move sh, w)
    }
    settled := cap.settle()
    fmt.println(settled.buckets[0])
}
