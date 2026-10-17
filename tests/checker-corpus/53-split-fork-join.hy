// ============================================================================
// 53-split-fork-join.hy — Checker corpus #53 (W-015 split spike).
//
// PURPOSE: A `split s at mid into a, b { ... }` block takes EXCLUSIVE
// ownership of the base slice (HYB-E3105 rejects live sharers), splits the
// base out for the whole block (HYB-E3106 rejects base uses inside), and
// defines two capacity-clipped forks whose writable ranges are provably
// disjoint by Go's full slice expression semantics (`s[0:mid:mid]` can never
// write past mid; `s[mid:len(s):cap(s)]` starts at mid). Disjoint ranges +
// exclusive base + no other handles => each fork can be moved into its own
// concurrent task without a data race (verified under `go run -race`).
//
// EXPECTED-VERDICT: ACCEPT
// ============================================================================

fn left(a []int, done chan int) {
  for i in 0..len(a) {
    a[i] = a[i] + 1
  }
  done <- 1
}

fn right(b []int, done chan int) {
  for i in 0..len(b) {
    b[i] = b[i] * 2
  }
  done <- 1
}

fn main() {
  s := make([]int, 8)
  for i in 0..len(s) {
    s[i] = i
  }
  mid := len(s) / 2
  done := make(chan int, 2)
  split s at mid into a, b {
    task left(move a, done)
    task right(move b, done)
  }
  <-done
  <-done
  fmt.println(s[0], s[7])
}
