// EXPECTED-VERDICT: REJECT HYB-E3201
// Split producer closes channel while peer split producer is still active.

fn main() {
    tx, rx := make(chan int, 2)
    tx1, tx2 := tx.split()
    task producer(move tx1)
    close(move tx2)
}
