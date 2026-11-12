struct box[t] {
    items: t
}

fn leak_nested_generic() {
    let shared: []map[string]int = [{"k": 1}]
    let b: box[[]map[string]int] = box[[]map[string]int]{ items: shared }
    let h = task() -> int {
        return len(b.items)
    }
    let _ = h.join()
}
