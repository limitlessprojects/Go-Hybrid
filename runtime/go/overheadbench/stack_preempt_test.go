package overheadbench

import (
	"runtime"
	"sync"
	"sync/atomic"
	"testing"
	"unsafe"

	"hybrid.program/hybridge"
)

// TestCrossShallowStackNeverBypassesGuardOnPreempt verifies that fast-path
// crossings (callFast1..4 and callFast1S..4S) never treat g.stackguard0 ==
// stackPreempt (0xfffffffffffffade) as multi-gigabyte stack headroom on a
// shallow goroutine stack during concurrent runtime.GC preemption.
func TestCrossShallowStackNeverBypassesGuardOnPreempt(t *testing.T) {
	var bypassed atomic.Bool
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

	// 1. Direct fast-path oracle (Cross1Raw has no Go prologue inside callFast1):
	// descend until real stack headroom sits strictly inside (0, fastStackNeedDefault-256),
	// then hammer Cross1Raw under concurrent GC preemption. Every call must return
	// needsDeep (^uint64(1)) rather than executing FnNop (77).
	var wg sync.WaitGroup
	for g := 0; g < 8; g++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			var descend func(depth int)
			descend = func(depth int) {
				var pad [512]byte
				pad[depth%512] = byte(depth)
				h := hybridge.SPHeadroomForTest()
				if h == 0 || h >= fastStackNeedDefault-256 {
					if depth < 64 {
						descend(depth + 1)
					}
					runtime.KeepAlive(&pad)
					return
				}
				for i := 0; i < 20_000 && !bypassed.Load(); i++ {
					if got := hybridge.Cross1Raw(FnNop, 77); got == 77 {
						bypassed.Store(true)
						break
					}
				}
				runtime.KeepAlive(&pad)
			}
			descend(0)
		}()
	}
	wg.Wait()

	// 2. Descend into (256, SmallGuard-128) where BOTH default (4096) and small
	// (1024) guards must reject the fast path and retry via deepPool unless Go
	// reallocates the goroutine stack (detected via frame-pointer relocation).
	var missedSmall atomic.Bool
	wg.Add(1)
	go func() {
		defer wg.Done()
		var descendSmall func(depth int)
		descendSmall = func(depth int) {
			var pad [256]byte
			pad[depth%256] = byte(depth)
			h := hybridge.SPHeadroomForTest()
			if h == 0 || h >= hybridge.SmallGuard-128 {
				if depth < 128 && (h == 0 || h >= hybridge.SmallGuard-128) {
					descendSmall(depth + 1)
				}
				runtime.KeepAlive(&pad)
				return
			}
			sp0 := uintptr(unsafe.Pointer(&pad[0]))
			for i := 0; i < 2_000; i++ {
				before := hybridge.DeepPathCount.Load()
				_ = hybridge.Cross1S(FnNop, 1)
				_ = hybridge.Cross2S(FnNop, 1, 2)
				_ = hybridge.Cross3S(FnNop, 1, 2, 3)
				_ = hybridge.Cross4S(FnNop, 1, 2, 3, 4)
				after := hybridge.DeepPathCount.Load()
				if uintptr(unsafe.Pointer(&pad[0])) != sp0 {
					break
				}
				if after-before < 4 {
					missedSmall.Store(true)
					break
				}
			}
			runtime.KeepAlive(&pad)
		}
		for round := 0; round < 30 && !missedSmall.Load(); round++ {
			descendSmall(0)
		}
	}()
	wg.Wait()

	stop.Store(true)
	gcWG.Wait()

	if bypassed.Load() {
		t.Fatal("callFast1 bypassed the 4096 B stack guard on a shallow stack during stackPreempt")
	}
	if missedSmall.Load() {
		t.Fatal("Cross1S..4S bypassed the 1024 B stack guard on a shallow stack during stackPreempt")
	}
}
