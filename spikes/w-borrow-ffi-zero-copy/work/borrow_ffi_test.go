package wborrowffizerocopy

import (
	"sync"
	"testing"
	"time"
	"unsafe"
)

type HandleRegistry struct {
	mu     sync.Mutex
	nextID uint64
	slots  map[uint64][]byte
}

func NewHandleRegistry() *HandleRegistry {
	return &HandleRegistry{slots: make(map[uint64][]byte)}
}

func (r *HandleRegistry) CallChecksumHandleTable(payload []byte) uint64 {
	r.mu.Lock()
	r.nextID++
	id := r.nextID
	buf := make([]byte, len(payload))
	copy(buf, payload)
	r.slots[id] = buf
	r.mu.Unlock()

	var sum uint64
	for i := 0; i < len(buf); i += 64 {
		sum += uint64(buf[i])
	}

	r.mu.Lock()
	delete(r.slots, id)
	r.mu.Unlock()
	return sum
}

// BorrowView Header represents the 16-byte (ptr, len) synchronous trampoline view.
type BorrowView struct {
	ptr unsafe.Pointer
	len int
}

//go:noinline
func CallChecksumBorrowFFI(clipped []byte) uint64 {
	v := BorrowView{ptr: unsafe.Pointer(unsafe.SliceData(clipped)), len: len(clipped)}
	s := unsafe.Slice((*byte)(v.ptr), v.len)
	var sum uint64
	for i := 0; i < len(s); i += 64 {
		sum += uint64(s[i])
	}
	return sum
}

func TestP1ZeroAllocsAndParity(t *testing.T) {
	reg := NewHandleRegistry()
	payload := make([]byte, 4096)
	for i := range payload {
		payload[i] = byte(i & 0xff)
	}
	clipped := payload[:len(payload):len(payload)]
	if got, want := CallChecksumBorrowFFI(clipped), reg.CallChecksumHandleTable(payload); got != want {
		t.Fatalf("checksum mismatch: got %d, want %d", got, want)
	}
	allocs := testing.AllocsPerRun(200, func() {
		_ = CallChecksumBorrowFFI(clipped)
	})
	if allocs != 0 {
		t.Fatalf("P1 falsified: expected 0 allocs/op on borrow_ffi path, got %v", allocs)
	}
}

func TestP3LatencySpeedupRatio(t *testing.T) {
	reg := NewHandleRegistry()
	payload := make([]byte, 4096)
	clipped := payload[:len(payload):len(payload)]
	const iters = 20000

	t0 := time.Now()
	for i := 0; i < iters; i++ {
		_ = reg.CallChecksumHandleTable(payload)
	}
	handleDur := time.Since(t0)

	t1 := time.Now()
	for i := 0; i < iters; i++ {
		_ = CallChecksumBorrowFFI(clipped)
	}
	borrowDur := time.Since(t1)
	if borrowDur <= 0 {
		borrowDur = 1
	}
	if handleDur < borrowDur*5 {
		t.Fatalf("P3 falsified: handle_table (%v) vs borrow_ffi (%v)", handleDur, borrowDur)
	}
}

func TestP4aFalsifierAppendReallocation(t *testing.T) {
	s := make([]byte, 4, 4)
	s[0] = 10
	pinnedPtr := unsafe.SliceData(s)
	// Simulate unchecked callback append reallocating backing array during borrow window:
	s = append(s, 20, 30, 40, 50)
	s[0] = 99
	staleView := unsafe.Slice(pinnedPtr, 4)
	if staleView[0] == s[0] {
		t.Fatalf("P4a expected reallocated backing array to diverge from stale pinned pointer")
	}
}

func TestP4bFalsifierSubsliceCapacityBleed(t *testing.T) {
	backing := []byte{1, 2, 3, 4, 0xAA, 0xBB}
	unclipped := backing[0:4]
	clipped := backing[0:4:4]
	if cap(unclipped) <= len(unclipped) {
		t.Fatalf("expected unclipped subslice to expose spare capacity")
	}
	if cap(clipped) != len(clipped) {
		t.Fatalf("P4b falsified: 3-index clip must enforce cap == len")
	}
}
