// EXPECTED-VERDICT: REJECT HYB-E3001
// Send after close on consumed affine send capability triggers use-after-move.

fn main() {
    tx, rx := make(chan int, 1)
    tx <- 10
    close(move tx)
    tx <- 20
}
