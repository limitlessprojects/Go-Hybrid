import "sync"

struct holder {
    mu sync.mutex
    @[sync_guarded(missing_lock)]
    m  map[string]int
}
