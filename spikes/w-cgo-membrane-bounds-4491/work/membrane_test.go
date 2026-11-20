package membrane

import (
	"sync"
	"testing"
	"unsafe"
)

func TestMembranePinLifecycle(t *testing.T) {
	buf := make([]byte, 1024)
	for i := range buf {
		buf[i] = byte(i % 256)
	}

	m := NewMembrane()
	ptr, length, err := m.PinSlice(buf)
	if err != nil {
		t.Fatalf("PinSlice failed: %v", err)
	}
	if ptr == nil || length != 1024 {
		t.Fatalf("Invalid ptr or length: ptr=%v, len=%d", ptr, length)
	}

	sum, err := MockForeignCProcess(ptr, length)
	if err != nil {
		t.Fatalf("MockForeignCProcess failed: %v", err)
	}
	if sum <= 0 {
		t.Fatalf("Unexpected sum: %d", sum)
	}

	m.Unpin()
}

func TestWithMembraneHelper(t *testing.T) {
	buf := []byte("Hybrid Foreign Membrane Test Buffer")
	res, err := WithMembrane(buf, func(ptr unsafe.Pointer, length int) (int, error) {
		return MockForeignCProcess(ptr, length)
	})
	if err != nil {
		t.Fatalf("WithMembrane failed: %v", err)
	}
	if res <= 0 {
		t.Fatalf("Expected positive result, got: %d", res)
	}
}

func TestConcurrentMembranes(t *testing.T) {
	const workers = 100
	const iterations = 500

	var wg sync.WaitGroup
	wg.Add(workers)

	for w := 0; w < workers; w++ {
		go func(workerID int) {
			defer wg.Done()
			for i := 0; i < iterations; i++ {
				data := make([]byte, 128)
				data[0] = byte(workerID)
				data[1] = byte(i)
				_, err := WithMembrane(data, func(ptr unsafe.Pointer, length int) (int, error) {
					return MockForeignCProcess(ptr, length)
				})
				if err != nil {
					t.Errorf("Concurrent WithMembrane failed: %v", err)
					return
				}
			}
		}(w)
	}

	wg.Wait()
}

func BenchmarkDirectCgoMembrane(b *testing.B) {
	buf := make([]byte, 512)
	b.ResetTimer()
	b.ReportAllocs()

	for i := 0; i < b.N; i++ {
		_, _ = WithMembrane(buf, func(ptr unsafe.Pointer, length int) (int, error) {
			return MockForeignCProcess(ptr, length)
		})
	}
}
