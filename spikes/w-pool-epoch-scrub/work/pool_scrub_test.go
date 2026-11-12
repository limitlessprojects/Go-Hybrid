package poolscrub

import (
	"runtime"
	"sync"
	"testing"
)

// TestConcurrentScrubbedPoolRaceFree verifies H1:
// 64 concurrent worker goroutines recycling 100,000 composite structs across
// Get -> mutate -> PutScrubbed epochs execute with 0 data races under `go test -race`
// and 0 cross-epoch stale data leaks.
func TestConcurrentScrubbedPoolRaceFree(t *testing.T) {
	const (
		workers    = 64
		iterations = 1600 // 64 * 1600 = 102,400 recycled composite structs
	)
	pool := NewTypedPoolPacket(16)
	var wg sync.WaitGroup
	wg.Add(workers)

	for w := 0; w < workers; w++ {
		workerID := w + 1
		go func() {
			defer wg.Done()
			localTarget := workerID * 1000
			for i := 0; i < iterations; i++ {
				pkt := pool.Get()

				// Verify epoch isolation on acquisition:
				if pkt.ScalarID != 0 {
					t.Errorf("epoch leak: expected ScalarID=0, got %d", pkt.ScalarID)
					return
				}
				if len(pkt.Payload) != 0 {
					t.Errorf("epoch leak: expected len(Payload)=0, got %d", len(pkt.Payload))
					return
				}
				if len(pkt.Meta) != 0 {
					t.Errorf("epoch leak: expected len(Meta)=0, got %d", len(pkt.Meta))
					return
				}
				if pkt.ForeignPtr != nil {
					t.Errorf("epoch leak: expected ForeignPtr=nil, got non-nil")
					return
				}

				// Mutate all interior reference fields in this worker's epoch:
				pkt.ScalarID = workerID
				pkt.Payload = append(pkt.Payload, workerID, i, workerID^i)
				pkt.Meta["worker"] = workerID
				pkt.Meta["seq"] = i
				pkt.ForeignPtr = &localTarget

				if pkt.Payload[0] != workerID || *pkt.ForeignPtr != localTarget {
					t.Errorf("corrupted in-epoch state")
					return
				}

				// Consume via O4.11b scrubbed Put:
				pool.PutScrubbed(pkt)
			}
		}()
	}

	wg.Wait()
}

// TestInteriorLaundromatIsolation verifies H1 & F1:
// Demonstrates that __hy_scrub_Packet deterministically zeroes backing-array elements,
// clips slice length to 0, clears map entries, and nils interior pointers so a subsequent
// epoch cannot observe or alias prior epoch state.
func TestInteriorLaundromatIsolation(t *testing.T) {
	pool := NewTypedPoolPacket(8)

	// Epoch 1: populate sensitive interior fields and a stack/heap pointer
	secret := 424242
	p1 := pool.Get()
	p1.ScalarID = 99
	p1.Payload = append(p1.Payload, 111, 222, 333)
	p1.Meta["token"] = 777
	p1.ForeignPtr = &secret

	// Return to pool with compiler-synthesized scrubber
	__hy_scrub_Packet(p1)

	if p1.ScalarID != 0 {
		t.Fatalf("expected ScalarID=0 after scrub, got %d", p1.ScalarID)
	}
	if len(p1.Payload) != 0 || cap(p1.Payload) != 8 {
		t.Fatalf("expected len=0 cap=8 after scrub, got len=%d cap=%d", len(p1.Payload), cap(p1.Payload))
	}
	// Verify backing array elements were cleared too (no stale data if resliced)
	rawReslice := p1.Payload[:3]
	for idx, v := range rawReslice {
		if v != 0 {
			t.Fatalf("expected backing array slot %d to be 0 after scrub, got %d", idx, v)
		}
	}
	if len(p1.Meta) != 0 {
		t.Fatalf("expected empty map after scrub, got len=%d", len(p1.Meta))
	}
	if p1.ForeignPtr != nil {
		t.Fatalf("expected ForeignPtr=nil after scrub, got %v", p1.ForeignPtr)
	}
}

// TestCapacityRetentionZeroAlloc verifies H2 & F2:
// Steady-state epoch scrubbing (__hy_scrub_Packet) preserves slice and map
// backing storage across 1,000 recycling epochs with strictly 0 heap allocations.
// (Note: under `go test -race`, Go's stdlib `sync.Pool.Put` intentionally drops
// 1-in-4 items via `runtime_randn(4) == 0` in `src/sync/pool.go`, so we test
// the scrubber + capacity reuse invariant deterministically here and measure full
// `sync.Pool` 0 B/op / 0 allocs/op in BenchmarkTypedPoolScrubbed).
func TestCapacityRetentionZeroAlloc(t *testing.T) {
	pool := NewTypedPoolPacket(32)
	pkt := pool.Get()
	pkt.Payload = append(pkt.Payload, 1, 2, 3, 4)
	pkt.Meta["k"] = 1
	__hy_scrub_Packet(pkt)

	allocs := testing.AllocsPerRun(1000, func() {
		pkt.ScalarID = 7
		pkt.Payload = append(pkt.Payload, 10, 20, 30, 40)
		pkt.Meta["k"] = 42
		__hy_scrub_Packet(pkt)
	})

	if allocs != 0 {
		t.Fatalf("F2 FALSIFIED: expected 0 allocs/op on steady-state epoch scrub/reuse, got %v allocs/op", allocs)
	}
	if len(pkt.Payload) != 0 || cap(pkt.Payload) != 32 {
		t.Fatalf("expected len=0 cap=32 after 1000 scrubbed epochs, got len=%d cap=%d", len(pkt.Payload), cap(pkt.Payload))
	}
	pool.PutScrubbed(pkt)
}

// BenchmarkTypedPoolScrubbed measures H2 & F2: steady-state O4.11b scrubbed pool cycle.
func BenchmarkTypedPoolScrubbed(b *testing.B) {
	pool := NewTypedPoolPacket(32)
	warm := pool.Get()
	warm.Payload = append(warm.Payload, 1, 2, 3, 4)
	pool.PutScrubbed(warm)

	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		pkt := pool.Get()
		pkt.ScalarID = i
		pkt.Payload = append(pkt.Payload, i, i+1, i+2, i+3)
		pool.PutScrubbed(pkt)
	}
}

// BenchmarkBaselineManualSyncPool measures hand-written unsafe Go sync.Pool for delta comparison.
func BenchmarkBaselineManualSyncPool(b *testing.B) {
	pool := NewTypedPoolPacket(32)
	warm := pool.Get()
	warm.Payload = append(warm.Payload, 1, 2, 3, 4)
	warm.Payload = warm.Payload[:0]
	pool.PutUnscrubbed(warm)

	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		pkt := pool.Get()
		pkt.ScalarID = i
		pkt.Payload = append(pkt.Payload, i, i+1, i+2, i+3)
		pkt.Payload = pkt.Payload[:0]
		pool.PutUnscrubbed(pkt)
	}
	runtime.KeepAlive(pool)
}
