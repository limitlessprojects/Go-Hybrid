// EXPECTED-VERDICT: ACCEPT
// Well-ordered affine channel with single owner move to close.

fn main() {
    tx, rx := make(chan int, 1)
    tx <- 42
    close(move tx)
}
