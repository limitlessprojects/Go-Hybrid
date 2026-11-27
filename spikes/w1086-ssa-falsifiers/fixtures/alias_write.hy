package main

fn sink(m map[string]int) {
    m["k"] = 1
}

fn main() {
    m := map[string]int{"k": 0}
    peer := m
    task sink(move m)
    peer["k"] = 2
}
