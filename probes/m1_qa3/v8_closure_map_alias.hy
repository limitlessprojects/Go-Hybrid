type cbmap = map[string]fn()

fn f(mm cbmap, done chan<- bool) {
    if g := mm["a"]; g != nil {
        g()
    }
    done <- true
}

fn main() {
    m := cbmap{}
    m["a"] = fn() bool { return true }
    done := make(chan bool, 1)
    task f(move m, done)
    <-done
}
