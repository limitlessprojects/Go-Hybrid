fn f(mm map[string]int, done chan<- bool) {
    mm["x"] = 1
    done <- true
}

fn main() {
    m := map[string]int{"x": 1}
    m2 := m
    done := make(chan bool, 1)
    task f(move m, done)
    m2["y"] = 2
    <-done
}
