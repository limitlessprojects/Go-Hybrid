package overheadbench

import (
	"fmt"
	"sync"
	"testing"
)

func TestNopPayloadFidelity(t *testing.T) {
	payloads := []uint64{0, 1, 42, 1 << 32, 0x68795f61626931, ^uint64(0) >> 1}
	for _, x := range payloads {
		if got := Nop(x); got != x {
			t.Fatalf("Nop(%#x) = %#x", x, got)
		}
		if v, ok := NopRaw(x); ok && v != x {
			t.Fatalf("NopRaw(%#x) = %#x", x, v)
		}
	}
}

func TestConcurrentCrossings(t *testing.T) {
	const workers = 8
	const perWorker = 500
	var wg sync.WaitGroup
	errs := make(chan error, workers)
	for w := 0; w < workers; w++ {
		wg.Add(1)
		go func(w int) {
			defer wg.Done()
			for i := 0; i < perWorker; i++ {
				want := uint64(w*perWorker + i)
				if got := Nop(want); got != want {
					errs <- fmt.Errorf("payload: got %#x, want %#x", got, want)
					return
				}
			}
		}(w)
	}
	wg.Wait()
	select {
	case err := <-errs:
		t.Fatal(err)
	default:
	}
}
