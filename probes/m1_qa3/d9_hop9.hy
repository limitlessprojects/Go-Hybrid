type m9 = m8
type m8 = m7
type m7 = m6
type m6 = m5
type m5 = m4
type m4 = m3
type m3 = m2
type m2 = m1
type m1 = map[string]int

fn f(mm map[string]int, done chan<- bool) {
    mm["x"] = 1
    done <- true
}

fn main() {
    m := m9{}
    m2 := m
    done := make(chan bool, 1)
    task f(move m, done)
    m2["x"] = 1
    <-done
}
