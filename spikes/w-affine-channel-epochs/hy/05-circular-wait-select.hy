// EXPECTED-VERDICT: REJECT HYB-E3202
// Cyclic wait-for dependency between chA and chB in nested select arms.

fn main() {
    chA := make(chan int, 1)
    chB := make(chan int, 1)
    select {
        case <-chB:
            chA <- 1
    }
}
