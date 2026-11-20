import "sync"

struct holder {
    mu sync.mutex
    @[sync_guarded(mu)]
    m  map[string]int
}

fn worker(h *holder, done chan bool) {
    for _ in 0..1000 {
        h.mu.lock()
        h.m["x"] = h.m["x"] + 1
        h.mu.unlock()
    }
    done <- true
}

fn main() {
    var h holder
    var done chan bool
    go worker(h, done)
    <-done
}
