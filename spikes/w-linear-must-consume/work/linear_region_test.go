package linearregion

import (
	"sync"
	"testing"
)

type noopSink struct{}

func (noopSink) Observe(v any) int {
	if p, ok := v.(*Packet); ok {
		return p.Len
	}
	return 0
}

func TestParityAndZeroAllocs(t *testing.T) {
	const N = 1000
	gotLinear := RunLinearStackLoop(N)
	gotPool := RunSyncPoolLoop(N)
	if gotLinear != gotPool {
		t.Fatalf("parity mismatch: linear=%d vs pool=%d", gotLinear, gotPool)
	}

	allocs := testing.AllocsPerRun(100, func() {
		_ = RunLinearStackLoop(256)
	})
	if allocs != 0 {
		t.Fatalf("expected 0 allocs/run for RunLinearStackLoop, got %v", allocs)
	}
}

func TestLinearHandleExactlyOnce(t *testing.T) {
	h := AcquireHandle(7, 42)
	if h.IsConsumed() {
		t.Fatal("fresh handle must not be marked consumed")
	}
	val, err := h.ConsumeClose()
	if err != nil || val != 42 {
		t.Fatalf("expected (42, nil), got (%d, %v)", val, err)
	}
	if !h.IsConsumed() {
		t.Fatal("handle must be marked consumed after ConsumeClose")
	}
	if _, err := h.ConsumeClose(); err == nil {
		t.Fatal("second ConsumeClose must fail with HYB-E3003")
	}
}

func TestConcurrentLinearStackLoopsRaceFree(t *testing.T) {
	var wg sync.WaitGroup
	results := make([]int64, 8)
	for w := 0; w < 8; w++ {
		w := w
		wg.Add(1)
		go func() {
			defer wg.Done()
			results[w] = RunLinearStackLoop(500)
		}()
	}
	wg.Wait()
	for w := 1; w < 8; w++ {
		if results[w] != results[0] {
			t.Fatalf("worker %d mismatch: %d != %d", w, results[w], results[0])
		}
	}
}

func TestFalsifierF2InterfaceEscape(t *testing.T) {
	n := RunFalsifierF2InterfaceEscape(10, noopSink{})
	if n != 10 {
		t.Fatalf("expected 10, got %d", n)
	}
}

func BenchmarkLinearStackLoop(b *testing.B) {
	b.ReportAllocs()
	for i := 0; i < b.N; i++ {
		_ = RunLinearStackLoop(256)
	}
}

func BenchmarkSyncPoolLoop(b *testing.B) {
	b.ReportAllocs()
	for i := 0; i < b.N; i++ {
		_ = RunSyncPoolLoop(256)
	}
}
