package main

fn sink(m map[string]int) {
    m["k"] = 1
}

fn main() {
    m := map[string]int{"k": 0}
    for i in 0..2 {
        _ = i
        task sink(move m)
    }
}
