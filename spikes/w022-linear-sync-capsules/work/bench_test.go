package w022

import (
	"sync"
	"sync/atomic"
	"testing"
)

func BenchmarkAtomicRawPointerSwap(b *testing.B) {
	var ptr atomic.Pointer[int]
	v1, v2 := 1, 2
	ptr.Store(&v1)
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		_ = ptr.Swap(&v2)
	}
}

func BenchmarkAtomicCapsuleSwapMove(b *testing.B) {
	type r0 struct{}
	v1, v2 := 1, 2
	capBox, _ := NewAtomicCapsule(Live[r0]{}, &v1)
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		type rStep struct{}
		_, _ = AtomicSwapMove(capBox, Live[rStep]{}, &v2)
	}
}

func BenchmarkSyncPoolRaw(b *testing.B) {
	p := sync.Pool{New: func() any { v := 42; return &v }}
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		x := p.Get().(*int)
		*x = i
		p.Put(x)
	}
}

func BenchmarkLinearPoolPutMove(b *testing.B) {
	p := NewLinearPool(func() *int { v := 42; return &v }, func(x *int) { *x = 0 })
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		type rBuf struct{}
		w := Live[rBuf]{}
		x := p.Get()
		*HyUse(w, x) = i
		_ = PoolPutMove(p, w, x)
	}
}

func BenchmarkMutexRaw(b *testing.B) {
	var mu sync.Mutex
	val := 0
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		mu.Lock()
		val++
		mu.Unlock()
	}
}

func BenchmarkMutexCapsuleWithLock(b *testing.B) {
	type rMu struct{}
	capMu, _ := NewMutexCapsule(Live[rMu]{}, 0)
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		capMu.WithLock(func(loan ScopedLoan[int]) {
			*loan.Get()++
		})
	}
}
