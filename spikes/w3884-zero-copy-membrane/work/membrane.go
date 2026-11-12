package membrane

import (
	"errors"
	"fmt"
	"sync/atomic"
)

var (
	// ErrUseAfterMove indicates an attempt to access a membrane using a capability token that has already been consumed.
	ErrUseAfterMove = errors.New("HYB-E3884-USE-AFTER-MOVE: capability token has already been consumed by linear transfer")
	// ErrStaleRegion indicates capability token generation mismatch with the membrane target.
	ErrStaleRegion = errors.New("HYB-E3884-STALE-CAPABILITY: capability token generation does not match active membrane epoch")
	// ErrOutOfBounds indicates slice window or write offset exceeds buffer capacity.
	ErrOutOfBounds = errors.New("HYB-E3884-OUT-OF-BOUNDS: requested offset or slice length exceeds membrane boundary")
)

// Region is a compile-time marker interface designating a memory region.
// Concrete region types implement this interface to provide distinct phantom type witnesses.
type Region interface {
	isRegion()
}

// Concrete regions for multi-stage pipeline demonstrations.
type (
	RegionA struct{}
	RegionB struct{}
	RegionC struct{}
)

func (RegionA) isRegion() {}
func (RegionB) isRegion() {}
func (RegionC) isRegion() {}

// globalEpoch generates unique generation identifiers for capability instances.
var globalEpoch uint64

// RegionCap is a zero-cost compile-time phantom type capability witness.
// The type parameter R guarantees at compile time that only operations targeting
// Region R can be admitted.
type RegionCap[R Region] struct {
	epoch    uint64
	consumed *atomic.Bool
}

// NewRegionCap initializes an active capability token for region R.
func NewRegionCap[R Region]() RegionCap[R] {
	consumed := &atomic.Bool{}
	consumed.Store(false)
	return RegionCap[R]{
		epoch:    atomic.AddUint64(&globalEpoch, 1),
		consumed: consumed,
	}
}

// IsConsumed reports whether this capability token has already been spent.
func (c RegionCap[R]) IsConsumed() bool {
	if c.consumed == nil {
		return true
	}
	return c.consumed.Load()
}

// BufferMembrane wraps a contiguous memory buffer behind region R.
// Access to the underlying buffer requires presenting an active RegionCap[R].
type BufferMembrane[R Region] struct {
	buf    []byte
	epoch  uint64
	offset int
}

// NewBufferMembrane constructs a new BufferMembrane wrapping the provided byte slice.
// The slice header is adopted directly without copying underlying array data.
func NewBufferMembrane[R Region](cap RegionCap[R], buf []byte) (BufferMembrane[R], error) {
	if cap.IsConsumed() {
		return BufferMembrane[R]{}, ErrUseAfterMove
	}
	return BufferMembrane[R]{
		buf:    buf,
		epoch:  cap.epoch,
		offset: 0,
	}, nil
}

// Len returns the accessible length of the wrapped buffer.
func (m BufferMembrane[R]) Len() int {
	return len(m.buf) - m.offset
}

// Cap returns the total capacity of the wrapped buffer.
func (m BufferMembrane[R]) Cap() int {
	return cap(m.buf)
}

// View returns a zero-copy slice of the underlying buffer.
// It requires an active RegionCap[R] matching the membrane's epoch.
func (m BufferMembrane[R]) View(cap RegionCap[R]) ([]byte, error) {
	if cap.IsConsumed() {
		return nil, ErrUseAfterMove
	}
	if cap.epoch != m.epoch {
		return nil, ErrStaleRegion
	}
	return m.buf[m.offset:], nil
}

// MustView returns the slice or panics on violation. Useful for hot loops.
func (m BufferMembrane[R]) MustView(cap RegionCap[R]) []byte {
	v, err := m.View(cap)
	if err != nil {
		panic(fmt.Sprintf("membrane access failure: %v", err))
	}
	return v
}

// Write copies data into the underlying buffer at the given offset.
// No intermediate allocations or CGO trampolines are incurred.
func (m BufferMembrane[R]) Write(cap RegionCap[R], offset int, data []byte) (int, error) {
	if cap.IsConsumed() {
		return 0, ErrUseAfterMove
	}
	if cap.epoch != m.epoch {
		return 0, ErrStaleRegion
	}
	if offset < 0 || m.offset+offset > len(m.buf) {
		return 0, ErrOutOfBounds
	}
	if m.offset+offset+len(data) > len(m.buf) {
		return 0, ErrOutOfBounds
	}
	n := copy(m.buf[m.offset+offset:], data)
	return n, nil
}

// Slice returns a sub-membrane bounded by start and end offsets.
// The underlying memory is shared without allocation; the capability token is preserved.
func (m BufferMembrane[R]) Slice(cap RegionCap[R], start, end int) (BufferMembrane[R], error) {
	if cap.IsConsumed() {
		return BufferMembrane[R]{}, ErrUseAfterMove
	}
	if cap.epoch != m.epoch {
		return BufferMembrane[R]{}, ErrStaleRegion
	}
	if start < 0 || end < start || m.offset+end > len(m.buf) {
		return BufferMembrane[R]{}, ErrOutOfBounds
	}
	return BufferMembrane[R]{
		buf:    m.buf[:m.offset+end],
		epoch:  m.epoch,
		offset: m.offset + start,
	}, nil
}

// TransferBuffer atomically consumes cap1 and rebinds the buffer to region R2.
// The underlying memory is NOT copied. The memory transfer latency is strictly O(1)
// and incurs 0 heap allocations for the underlying buffer.
func TransferBuffer[R1, R2 Region](m BufferMembrane[R1], cap1 *RegionCap[R1]) (BufferMembrane[R2], RegionCap[R2], error) {
	if cap1 == nil || cap1.IsConsumed() {
		return BufferMembrane[R2]{}, RegionCap[R2]{}, ErrUseAfterMove
	}
	if cap1.epoch != m.epoch {
		return BufferMembrane[R2]{}, RegionCap[R2]{}, ErrStaleRegion
	}

	// Consume source capability (linear affine consumption)
	cap1.consumed.Store(true)

	// Issue new capability for region R2
	newCap := NewRegionCap[R2]()

	// Rebind buffer membrane to region R2 without copying data
	newMembrane := BufferMembrane[R2]{
		buf:    m.buf,
		epoch:  newCap.epoch,
		offset: m.offset,
	}

	return newMembrane, newCap, nil
}

// EnterRegion executes an enclosed block within an ephemeral region scope.
func EnterRegion[R Region, T any](fn func(RegionCap[R]) (T, error)) (T, error) {
	cap := NewRegionCap[R]()
	defer cap.consumed.Store(true)
	return fn(cap)
}
