type mmap = map[string]int

fn f(mm map[string]int, done chan<- bool) {
    mm["x"] = 1
    done <- true
}

fn main() {
    m := mmap{}
    m2 := m
    done := make(chan bool, 1)
    task f(move m, done)
    m2["y"] = 2
    <-done
}
