import "sync"

struct holder {
    mu sync.mutex
    m  map[string]int
}

fn worker(h *holder, done chan bool) {
    h.mu.lock()
    h.m["x"] = 1
    h.mu.unlock()
    done <- true
}

fn main() {
    var h holder
    var done chan bool
    // Fails because m is mutable and not declared with @[sync_guarded]
    go worker(h, done)
}
