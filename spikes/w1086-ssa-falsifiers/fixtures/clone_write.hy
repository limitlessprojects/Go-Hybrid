package main

fn sink(m map[string]int) {
    m["k"] = 1
}

fn main() {
    m := map[string]int{"k": 0}
    peer := map[string]int{}
    for k, value in m {
        peer[k] = value
    }
    task sink(move m)
    peer["k"] = 2
}
