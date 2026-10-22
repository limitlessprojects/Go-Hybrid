// 1. Channel receive: passes hyb check with 0 diagnostics, fails go build
fn main() {
    ch := make(chan int, 1)
    a, b, c := <-ch
    _ = a
    _ = b
    _ = c
}
