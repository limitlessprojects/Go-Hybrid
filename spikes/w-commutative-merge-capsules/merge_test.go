package mergecapsule

import (
	"sync"
	"testing"
	"time"
)

type Metrics struct {
	Hits       uint64
	TotalBytes uint64
	MaxLatency int64
	Errors     uint64
}

func combineMetrics(a *Metrics, b Metrics) {
	a.Hits += b.Hits
	a.TotalBytes += b.TotalBytes
	if b.MaxLatency > a.MaxLatency {
		a.MaxLatency = b.MaxLatency
	}
	a.Errors += b.Errors
}

func TestRaceFreedom(t *testing.T) {
	capsule := New(Metrics{}, combineMetrics)
	numWorkers := 64
	updatesPerWorker := 10_000

	var wg sync.WaitGroup
	wg.Add(numWorkers)

	for i := 0; i < numWorkers; i++ {
		shard := capsule.Split()
		go func(s *Shard[Metrics], workerID int) {
			defer wg.Done()
			defer s.MarkCompleted()
			for j := 0; j < updatesPerWorker; j++ {
				// Pure task-local mutation: 0 locks, 0 atomics, 0 channels
				s.Val.Hits++
				s.Val.TotalBytes += uint64(j)
				if int64(j) > s.Val.MaxLatency {
					s.Val.MaxLatency = int64(j)
				}
			}
		}(shard, i)
	}

	wg.Wait()
	res, err := capsule.Settle()
	if err != nil {
		t.Fatalf("unexpected settle error: %v", err)
	}

	expectedHits := uint64(numWorkers * updatesPerWorker)
	if res.Hits != expectedHits {
		t.Errorf("expected %d hits, got %d", expectedHits, res.Hits)
	}

	if res.MaxLatency != int64(updatesPerWorker-1) {
		t.Errorf("expected max latency %d, got %d", updatesPerWorker-1, res.MaxLatency)
	}
}

func TestDeterminism(t *testing.T) {
	var firstResult *Metrics

	for run := 0; run < 10; run++ {
		capsule := New(Metrics{}, combineMetrics)
		var wg sync.WaitGroup
		numWorkers := 16

		wg.Add(numWorkers)
		for i := 0; i < numWorkers; i++ {
			shard := capsule.Split()
			go func(s *Shard[Metrics], id int) {
				defer wg.Done()
				// Asymmetric work per shard
				for j := 0; j < (id+1)*500; j++ {
					s.Val.Hits++
					s.Val.TotalBytes += uint64(j * (id + 1))
					if int64(j) > s.Val.MaxLatency {
						s.Val.MaxLatency = int64(j)
					}
				}
			}(shard, i)
		}

		wg.Wait()
		res, err := capsule.Settle()
		if err != nil {
			t.Fatalf("run %d settle error: %v", run, err)
		}

		if firstResult == nil {
			firstResult = &res
		} else {
			if *firstResult != res {
				t.Fatalf("run %d produced non-deterministic result: %+v != %+v", run, *firstResult, res)
			}
		}
	}
}

func TestDeadlockAndEarlyCancelImmunity(t *testing.T) {
	capsule := New(Metrics{}, combineMetrics)
	var wg sync.WaitGroup
	numWorkers := 8

	wg.Add(numWorkers)
	for i := 0; i < numWorkers; i++ {
		shard := capsule.Split()
		go func(s *Shard[Metrics], id int) {
			defer wg.Done()
			select {
			case <-capsule.Context().Done():
				// Worker stopped early
				return
			case <-time.After(5 * time.Millisecond):
				s.Val.Hits += 100
			}
		}(shard, i)
	}

	// Trigger early abort from parent / worker
	capsule.Abort(nil)
	wg.Wait()

	// Capsule settles cleanly without wedging or leaking goroutines
	_, err := capsule.Settle()
	if err != nil {
		t.Fatalf("failed to settle cancelled capsule: %v", err)
	}
}

// ---------------------------------------------------------------------------
// Benchmarks
// ---------------------------------------------------------------------------

func BenchmarkMergeCapsule_8Workers(b *testing.B) {
	benchmarkMergeCapsule(b, 8)
}

func BenchmarkMergeCapsule_64Workers(b *testing.B) {
	benchmarkMergeCapsule(b, 64)
}

func BenchmarkMutex_8Workers(b *testing.B) {
	benchmarkMutex(b, 8)
}

func BenchmarkMutex_64Workers(b *testing.B) {
	benchmarkMutex(b, 64)
}

func BenchmarkChannel_8Workers(b *testing.B) {
	benchmarkChannel(b, 8)
}

func BenchmarkChannel_64Workers(b *testing.B) {
	benchmarkChannel(b, 64)
}

func benchmarkMergeCapsule(b *testing.B, workers int) {
	b.ResetTimer()
	for n := 0; n < b.N; n++ {
		capsule := New(Metrics{}, combineMetrics)
		var wg sync.WaitGroup
		wg.Add(workers)

		for i := 0; i < workers; i++ {
			shard := capsule.Split()
			go func(s *Shard[Metrics]) {
				defer wg.Done()
				for j := 0; j < 1000; j++ {
					s.Val.Hits++
					s.Val.TotalBytes += 64
				}
			}(shard)
		}
		wg.Wait()
		capsule.Settle()
	}
}

func benchmarkMutex(b *testing.B, workers int) {
	b.ResetTimer()
	for n := 0; n < b.N; n++ {
		var mu sync.Mutex
		var m Metrics
		var wg sync.WaitGroup
		wg.Add(workers)

		for i := 0; i < workers; i++ {
			go func() {
				defer wg.Done()
				for j := 0; j < 1000; j++ {
					mu.Lock()
					m.Hits++
					m.TotalBytes += 64
					mu.Unlock()
				}
			}()
		}
		wg.Wait()
	}
}

func benchmarkChannel(b *testing.B, workers int) {
	type update struct {
		bytes uint64
	}
	b.ResetTimer()
	for n := 0; n < b.N; n++ {
		ch := make(chan update, 1024)
		var m Metrics
		var wg sync.WaitGroup
		var collectorWg sync.WaitGroup

		collectorWg.Add(1)
		go func() {
			defer collectorWg.Done()
			for u := range ch {
				m.Hits++
				m.TotalBytes += u.bytes
			}
		}()

		wg.Add(workers)
		for i := 0; i < workers; i++ {
			go func() {
				defer wg.Done()
				for j := 0; j < 1000; j++ {
					ch <- update{bytes: 64}
				}
			}()
		}
		wg.Wait()
		close(ch)
		collectorWg.Wait()
	}
}
