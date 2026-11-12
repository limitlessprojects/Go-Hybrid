import "sync"

struct holder {
    mu sync.mutex
    @[sync_guarded(mu)]
    m  map[string]int
}

fn worker(h *holder, done chan bool) {
    h.mu.lock()
    h.mu.unlock()
    // Unsound: lock was already released
    h.m["x"] = 99
    done <- true
}
