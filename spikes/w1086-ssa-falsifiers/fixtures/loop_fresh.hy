package main

fn sink(m map[string]int) {
    m["k"] = 1
}

fn main() {
    for i in 0..2 {
        m := map[string]int{"k": i}
        task sink(move m)
    }
}
