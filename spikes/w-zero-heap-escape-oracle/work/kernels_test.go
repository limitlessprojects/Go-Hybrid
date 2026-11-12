package noheap

import (
	"runtime"
	"runtime/debug"
	"slices"
	"sync/atomic"
	"testing"
	"time"
)

func TestOracleMatchesRuntimeAllocs(t *testing.T) {
	src := make([]byte, 4096)
	src[0] = 7
	dst := make([]byte, 4096)
	ints := []int64{1, 2, 3, 4}
	smallSlice := []int{10, 20}
	allocatingCb := func(v int) int {
		b := make([]byte, v+64)
		b[0] = byte(v)
		return int(b[0])
	}

	for _, tc := range []struct {
		name string
		fn   func()
	}{
		{"K1FixedScratch", func() { _ = K1FixedScratch(ints) }},
		{"K2CallerScratch", func() { _ = K2CallerScratch(dst, src) }},
		{"K3Const32KiB", func() { _ = K3Const32KiB(3) }},
		{"K4OpenCodedDefer", func() { _ = K4OpenCodedDefer(9) }},
		{"K5NonLeakingAny", func() { _ = K5NonLeakingAny(257) }},
		{"K6WindowChecksum", func() { _ = K6WindowChecksum(src) }},
		{"K7InlinedCleanHelper", func() { _ = K7InlinedCleanHelper(src) }},
	} {
		if got := testing.AllocsPerRun(200, tc.fn); got != 0 {
			t.Fatalf("%s expected 0 allocs/op, got %v", tc.name, got)
		}
	}

	for _, tc := range []struct {
		name string
		fn   func()
	}{
		{"F1InterfaceBox", func() { _ = F1InterfaceBox(257) }},
		{"F2DynamicMake", func() { _ = F2DynamicMake(src, 4096) }},
		{"F3Const128KiB", func() { _ = F3Const128KiB(3) }},
		{"F4LoopDefer", func() { _ = F4LoopDefer(4) }},
		{"F5ReturnedClosure", func() { _ = F5ReturnedClosure(5)() }},
		{"F6SliceAppend", func() { _ = F6SliceAppend(8) }},
		{"F7TransitiveCallee", func() { _ = F7TransitiveCallee(256) }},
		{"F8IndirectInterface", func() { _ = F8IndirectInterface(allocReader{}) }},
		{"F9OpenDeferPlusFuncVal", func() { _ = F9OpenDeferPlusFuncVal(3, allocatingCb) }},
		{"F10OpenDeferAllocFuncN", func() { _ = F10OpenDeferAllocFuncN(256) }},
		{"F11RecoverBoundsTrap", func() { _ = F11RecoverBoundsTrap(smallSlice, 9) }},
		{"F12InlinedHelperHyLine", func() { _ = F12InlinedHelperHyLine(nil) }},
	} {
		if got := testing.AllocsPerRun(200, tc.fn); got < 1 {
			t.Fatalf("%s expected >=1 allocs/op, got %v", tc.name, got)
		}
	}
}

func TestTailLatencyUnderGCPressure(t *testing.T) {
	prev := debug.SetGCPercent(10)
	defer debug.SetGCPercent(prev)
	var stop atomic.Bool
	done := make(chan struct{})
	go func() {
		defer close(done)
		for !stop.Load() {
			_ = make([]byte, 16*1024)
			runtime.Gosched()
		}
	}()
	defer func() { stop.Store(true); <-done }()

	const N = 20000
	src, dst := make([]byte, 4096), make([]byte, 4096)
	latHeap, latNoHeap := make([]int64, N), make([]int64, N)
	for i := 0; i < N; i++ {
		t0 := time.Now()
		_ = F2DynamicMake(src, 4096)
		latHeap[i] = time.Since(t0).Nanoseconds()
	}
	for i := 0; i < N; i++ {
		t0 := time.Now()
		_ = K2CallerScratch(dst, src)
		latNoHeap[i] = time.Since(t0).Nanoseconds()
	}
	slices.Sort(latHeap)
	slices.Sort(latNoHeap)
	t.Logf("GOGC=10 N=%d | F2 p50=%dns p99=%dns p999=%dns | K2(@[noheap]) p50=%dns p99=%dns p999=%dns",
		N, latHeap[N/2], latHeap[N*99/100], latHeap[N*999/1000],
		latNoHeap[N/2], latNoHeap[N*99/100], latNoHeap[N*999/1000])
}
