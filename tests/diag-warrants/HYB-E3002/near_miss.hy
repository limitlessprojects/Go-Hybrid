fn consume(m map[string]int) {} fn main() { m := map[string]int{}; consume(clone(m)); _ = m["k"] }
