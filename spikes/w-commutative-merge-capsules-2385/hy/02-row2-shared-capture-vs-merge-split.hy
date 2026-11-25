// 02-row2-shared-capture-vs-merge-split.hy (#2385)
// BEFORE: Row 2 (`HYB-E3001`) rejects capturing a mutable accumulator across tasks.
// AFTER: Each task receives an affine `shard[Histogram]` via `move sh`.
struct Histogram { total int }
fn add_shard(mut s shard[Histogram], v int) {
    s.total += v
    s.finish()
}
fn main() {
    mut cap := merge[Histogram]{}
    s1 := cap.split(); s2 := cap.split()
    task add_shard(move s1, 10)
    task add_shard(move s2, 20)
    res := cap.settle()
    fmt.println(res.total)
}
