package hybridge

import (
	"runtime"
	"sync"
	"sync/atomic"
	"testing"
)

// TestSPHeadroomClampsStackPreempt verifies that asynchronous preemption
// (g.stackguard0 == stackPreempt = 0xfffffffffffffade during runtime.GC)
// clamps SPHeadroomForTest() to 0 rather than wrapping SP - g.stackguard0
// into a multi-gigabyte unsigned difference on a shallow 2 KiB stack.
func TestSPHeadroomClampsStackPreempt(t *testing.T) {
	const maxFreshStack = 64 << 10
	var bogus atomic.Uintptr
	var stop atomic.Bool
	var gcWG sync.WaitGroup
	gcWG.Add(1)
	go func() {
		defer gcWG.Done()
		for !stop.Load() {
			runtime.GC()
			runtime.Gosched()
		}
	}()

	var wg sync.WaitGroup
	for g := 0; g < 8; g++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			for i := 0; i < 200_000 && bogus.Load() == 0; i++ {
				if h := SPHeadroomForTest(); h > maxFreshStack {
					bogus.Store(h)
					return
				}
			}
		}()
	}
	wg.Wait()
	stop.Store(true)
	gcWG.Wait()

	if got := bogus.Load(); got != 0 {
		t.Fatalf("SPHeadroomForTest() wrapped to %#x (%d bytes) on a 2 KiB goroutine stack under preemption", got, got)
	}
}
