// EXPECTED-VERDICT: REJECT HYB-E3001
// Double close of affine send capability violates linear ownership.

fn main() {
    tx, rx := make(chan int, 1)
    tx <- 1
    close(move tx)
    close(tx)
}
