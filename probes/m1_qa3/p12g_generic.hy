type gb = holder[int]

fn f(mm map[string]int, done chan<- bool) {
    mm["x"] = 1
    done <- true
}

fn main() {
    m := gb{}
    m2 := m
    done := make(chan bool, 1)
    task f(move m, done)
    m2["x"] = 1
    <-done
}
