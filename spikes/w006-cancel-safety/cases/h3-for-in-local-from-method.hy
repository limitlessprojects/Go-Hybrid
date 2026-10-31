// EXPECTED-CANCEL: E8002 x1
// `for … in` over a local bound from a method returning a channel.
struct src {
    c chan int
}
fn (s src) stream() chan int {
    return s.c
}
fn worker(done chan bool) {
    s := src{c: make(chan int)}
    ch := s.stream()
    for v in ch {
        fmt.println("{v}")
    }
    <-done
}
fn main() {
    done := make(chan bool)
    task worker(done)
    close(done)
}
