import "sync"

struct holder {
    mu sync.mutex
    @[sync_guarded(mu)]
    m  map[string]int
}

fn worker(h *holder, done chan bool) {
    // Unsound: access without holding mu
    h.m["x"] = 42
    done <- true
}
