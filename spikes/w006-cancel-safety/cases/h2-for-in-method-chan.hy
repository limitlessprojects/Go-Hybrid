// EXPECTED-CANCEL: E8002 x1
// `for … in` over a channel returned by a method.
struct src {
    c chan int
}
fn (s src) stream() chan int {
    return s.c
}
fn worker(done chan bool) {
    s := src{c: make(chan int)}
    for v in s.stream() {
        fmt.println("{v}")
    }
    <-done
}
fn main() {
    done := make(chan bool)
    task worker(done)
    close(done)
}
