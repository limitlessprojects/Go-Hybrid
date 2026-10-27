// kubernetes/82550 with the stored-borrow removed (value field instead of *T),
// so the only remaining question is the method spawn itself.
struct caching_provider {
    cache map[string]int
}

fn (d *caching_provider) provide() map[string]int {
    return map[string]int{}
}

struct lazy_provider {
    actual caching_provider
}

fn (p *lazy_provider) lazy_provide() int {
    m := p.actual.provide()
    return len(m)
}

fn main() {
    p := lazy_provider{}
    for i in [1, 2, 3, 4, 5, 6, 7, 8, 9, 10] {
        _ = i
        task p.lazy_provide()
    }
}