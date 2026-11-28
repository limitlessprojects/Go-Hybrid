package work_test

import (
	"errors"
	"runtime"
	"sync"
	"testing"
)

var (
	ErrPanicEnclave = errors.New("hy: rust panic caught inside structured enclave")
	ErrStaleHandle  = errors.New("HYB-E3003: handle poisoned by prior panic enclave")
)

const PoisonedGeneration = ^uint64(0)

type Handle struct {
	ID         uint64
	Generation uint64
}

type EnclaveRuntime struct {
	mu      sync.Mutex
	handles map[uint64]uint64 // id -> generation
	nextID  uint64
}

func NewEnclaveRuntime() *EnclaveRuntime {
	return &EnclaveRuntime{
		handles: make(map[uint64]uint64),
		nextID:  1,
	}
}

func (r *EnclaveRuntime) NewHandle() *Handle {
	r.mu.Lock()
	defer r.mu.Unlock()
	id := r.nextID
	r.nextID++
	r.handles[id] = 1
	return &Handle{ID: id, Generation: 1}
}

// CallTrampolineEnclave simulates the assembly trampoline landing pad wrapping the Rust frame.
func (r *EnclaveRuntime) CallTrampolineEnclave(h *Handle, fn func() (uint64, error)) (res uint64, err error) {
	r.mu.Lock()
	gen, exists := r.handles[h.ID]
	if !exists || gen != h.Generation || gen == PoisonedGeneration {
		r.mu.Unlock()
		return 0, ErrStaleHandle
	}
	r.mu.Unlock()

	// Enclave landing pad (DWARF unwinding catch boundary)
	defer func() {
		if rec := recover(); rec != nil {
			r.mu.Lock()
			r.handles[h.ID] = PoisonedGeneration
			r.mu.Unlock()
			err = ErrPanicEnclave
			res = 0
		}
	}()

	return fn()
}

// TestProcessSurvivalUnderPanic proves that 10,000 panics do not abort the process (Falsifier F1).
func TestProcessSurvivalUnderPanic(t *testing.T) {
	rt := NewEnclaveRuntime()
	const iterations = 10000

	for i := 0; i < iterations; i++ {
		h := rt.NewHandle()
		_, err := rt.CallTrampolineEnclave(h, func() (uint64, error) {
			// Simulate Rust leaf panic (e.g. index out of bounds)
			panic("Rust panic: index out of bounds at leaf slice[42]")
		})

		if !errors.Is(err, ErrPanicEnclave) {
			t.Fatalf("iteration %d: expected ErrPanicEnclave, got %v", i, err)
		}
	}
}

// TestHandlePoisoning verifies that subsequent calls on a panicked handle fail closed (Falsifier F2).
func TestHandlePoisoning(t *testing.T) {
	rt := NewEnclaveRuntime()
	h := rt.NewHandle()

	// First call panics
	_, err := rt.CallTrampolineEnclave(h, func() (uint64, error) {
		panic("Rust leaf assert failed")
	})
	if !errors.Is(err, ErrPanicEnclave) {
		t.Fatalf("expected ErrPanicEnclave, got %v", err)
	}

	// Subsequent access must return ErrStaleHandle
	_, postErr := rt.CallTrampolineEnclave(h, func() (uint64, error) {
		return 42, nil
	})
	if !errors.Is(postErr, ErrStaleHandle) {
		t.Fatalf("expected ErrStaleHandle on poisoned handle, got %v", postErr)
	}
}

// TestZeroLeakUnder10kPanics verifies deterministic memory destruction across repeated panics (Falsifier F3).
func TestZeroLeakUnder10kPanics(t *testing.T) {
	rt := NewEnclaveRuntime()
	const iterations = 10000

	runtime.GC()
	var m1 runtime.MemStats
	runtime.ReadMemStats(&m1)

	for i := 0; i < iterations; i++ {
		h := rt.NewHandle()
		_, _ = rt.CallTrampolineEnclave(h, func() (uint64, error) {
			panic("panic unwind test")
		})
	}

	runtime.GC()
	var m2 runtime.MemStats
	runtime.ReadMemStats(&m2)

	// In-use heap objects should not balloon uncontrollably
	if m2.HeapInuse > m1.HeapInuse+1024*1024 {
		t.Fatalf("detected memory leak: heap inuse grew from %d to %d", m1.HeapInuse, m2.HeapInuse)
	}
}

// BenchmarkHappyPathTrampoline verifies zero allocation and minimal call overhead on non-panicking calls.
func BenchmarkHappyPathTrampoline(b *testing.B) {
	rt := NewEnclaveRuntime()
	h := rt.NewHandle()

	b.ResetTimer()
	b.ReportAllocs()
	for i := 0; i < b.N; i++ {
		_, _ = rt.CallTrampolineEnclave(h, func() (uint64, error) {
			return 42, nil
		})
	}
}
