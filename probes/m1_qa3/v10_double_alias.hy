type mmap = map[string]int
type mmap2 = mmap

fn f(mm map[string]int, done chan<- bool) {
    mm["x"] = 1
    done <- true
}

fn main() {
    m := mmap2{}
    m2 := m
    m3 := m2
    done := make(chan bool, 1)
    task f(move m, done)
    m2["y"] = 2
    m3["z"] = 3
    <-done
}
