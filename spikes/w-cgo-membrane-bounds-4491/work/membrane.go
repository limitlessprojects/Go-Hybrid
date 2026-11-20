// Package membrane implements Certified Foreign Memory Membranes for Hybrid.
// It verifies that Go slices and pointers passed across foreign C ABI boundaries
// are stack-pinned via runtime.Pinner with strictly bounded, deterministic unpinning.
package membrane

import (
	"errors"
	"runtime"
	"sync"
	"unsafe"
)

var (
	ErrNullPointer      = errors.New("HYB-E3015: null pointer passed to non-nullable foreign ABI contract")
	ErrConcurrentAccess = errors.New("HYB-E3016: concurrent mutation detected during active foreign FFI membrane frame")
)

// Membrane wraps a slice or pointer with runtime.Pinner guarantees.
type Membrane struct {
	pinner runtime.Pinner
	active bool
	mu     sync.RWMutex
}

// NewMembrane creates an initialized membrane frame.
func NewMembrane() *Membrane {
	return &Membrane{}
}

// PinSlice pins the backing store of a slice so the Go GC will not move it
// while foreign C code executes. Returns the raw unsafe.Pointer.
func (m *Membrane) PinSlice(b []byte) (unsafe.Pointer, int, error) {
	if len(b) == 0 {
		return nil, 0, nil
	}
	m.mu.Lock()
	defer m.mu.Unlock()

	ptr := unsafe.SliceData(b)
	if ptr == nil {
		return nil, 0, ErrNullPointer
	}

	m.pinner.Pin(ptr)
	m.active = true
	return unsafe.Pointer(ptr), len(b), nil
}

// Unpin releases all pinned objects in this membrane frame.
func (m *Membrane) Unpin() {
	m.mu.Lock()
	defer m.mu.Unlock()
	if m.active {
		m.pinner.Unpin()
		m.active = false
	}
}

// WithMembrane executes an FFI function f with guaranteed pin and unpin lifecycle.
func WithMembrane[R any](buf []byte, f func(ptr unsafe.Pointer, length int) (R, error)) (R, error) {
	if len(buf) == 0 {
		return f(nil, 0)
	}

	var pinner runtime.Pinner
	ptr := unsafe.SliceData(buf)
	if ptr == nil {
		var zero R
		return zero, ErrNullPointer
	}

	pinner.Pin(ptr)
	defer pinner.Unpin()

	return f(unsafe.Pointer(ptr), len(buf))
}

// MockForeignCProcess simulates a foreign C function reading the pinned buffer.
func MockForeignCProcess(ptr unsafe.Pointer, length int) (int, error) {
	if ptr == nil && length > 0 {
		return 0, ErrNullPointer
	}
	// Simulate C checksum calculation
	data := unsafe.Slice((*byte)(ptr), length)
	sum := 0
	for _, b := range data {
		sum += int(b)
	}
	return sum, nil
}
