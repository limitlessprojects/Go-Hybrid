struct holder {
    v int
}
type halias = holder

fn f(h holder, done chan<- bool) {
    h.v = 1
    done <- true
}

fn main() {
    h := halias{v: 1}
    h2 := h
    done := make(chan bool, 1)
    task f(move h, done)
    h2.v = 2
    <-done
}
