package work

import (
	"sync"
	"testing"
)

// Emitted Go equivalent of:
//
//	struct holder {
//	    mu sync.mutex
//	    @[sync_guarded(mu)]
//	    m  map[string]int
//	}
type Holder struct {
	mu sync.Mutex
	m  map[string]int
}

func newHolder() *Holder {
	return &Holder{
		m: make(map[string]int),
	}
}

// Emitted Go equivalent of worker with static lock obligation
func worker(h *Holder, iters int, wg *sync.WaitGroup) {
	defer wg.Done()
	for i := 0; i < iters; i++ {
		h.mu.Lock()
		h.m["counter"]++
		h.mu.Unlock()
	}
}

func TestGuardedConcurrentWorkerPoolRaceSafety(t *testing.T) {
	h := newHolder()
	numWorkers := 8
	iters := 50000
	var wg sync.WaitGroup

	for i := 0; i < numWorkers; i++ {
		wg.Add(1)
		go worker(h, iters, &wg)
	}

	wg.Wait()

	h.mu.Lock()
	finalCount := h.m["counter"]
	h.mu.Unlock()

	expected := numWorkers * iters
	if finalCount != expected {
		t.Fatalf("expected counter %d, got %d", expected, finalCount)
	}
}

func BenchmarkGuardedFieldAccess(b *testing.B) {
	h := newHolder()
	b.ResetTimer()
	b.ReportAllocs()

	for i := 0; i < b.N; i++ {
		h.mu.Lock()
		h.m["val"] = i
		_ = h.m["val"]
		h.mu.Unlock()
	}
}
