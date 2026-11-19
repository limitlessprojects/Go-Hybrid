// 07-lru-generics — stresses: generics with monomorphization-friendly
// constraints, struct methods, Option instead of nil. TARGET SYNTAX.

struct lru_cache[k comparable, v any] {
    cap     int
    entries map[k]v
    order   []k                     // simplest possible LRU order tracking
}

fn lru_cache.new[k comparable, v any](cap int) lru_cache[k, v] {
    return lru_cache[k, v]{cap: cap, entries: map[k]v{}, order: []k{}}
}

fn (c *lru_cache[k, v]) get(key k) option[v] {   // no nil: some(v) or none
    if v, ok := c.entries[key]; ok {
        c.touch(key)
        return some(v)
    }
    return none
}

fn (c *lru_cache[k, v]) put(key k, value v) {
    if _, ok := c.entries[key]; !ok && len(c.order) == c.cap {
        evict := c.order[0]                        // evict LRU
        c.order = c.order[1:]
        delete(c.entries, evict)
    }
    c.entries[key] = value
    c.touch(key)
}

fn (c *lru_cache[k, v]) touch(key k) {
    // move key to back of order (most recently used)
    for i, e in c.order {
        if e == key {
            c.order = append(c.order[:i], c.order[i+1:]...)
            break
        }
    }
    c.order = append(c.order, key)
}

fn main() {
    c := lru_cache.new[string, int](2)             // instantiates for (string, int)
    c.put("a", 1)
    c.put("b", 2)
    _ = c.get("a")                                 // "a" becomes most recent
    c.put("c", 3)                                  // evicts "b"

    match c.get("b") {
        some(v): fmt.println("b={v}")              // NOT taken
        none:     fmt.println("b evicted")
    }
    match c.get("a") {
        some(v): fmt.println("a={v}")
        none:     fmt.println("a evicted")
    }
}
