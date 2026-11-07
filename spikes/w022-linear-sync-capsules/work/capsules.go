// Package w022 prototypes W-022: Linear Synchronization Capsules that flip
// GUARANTEES.md Rows 8 (managed mutex/atomic hatches), 10 (sync.Pool), and 11
// (sync/atomic.Pointer[T]) from "No soundness claim" into Tier-0 checked
// capabilities using O5 move-freeze phantom witnesses and O2 scoped loan guards.
package w022

import (
	"sync"
	"sync/atomic"
)

// Live[R] and Moved[R] are zero-size phantom capability witnesses (matching the
// W-004 witness vocabulary so W-004 and W-022 compose directly in hyb emit).
type Live[R any] struct{}
type Moved[R any] struct{}

// HyUse asserts at compile time that root R is still Live at a read/write site.
func HyUse[R any, T any](_ Live[R], v T) T { return v }

// -----------------------------------------------------------------------------
// 1. AtomicCapsule[T] — Flips GUARANTEES Row 11 (sync/atomic.Pointer[T])
// -----------------------------------------------------------------------------
// Why PR #52 rejected atomic.Pointer[T] as class-2 sendable (HYB-E3005):
// atomic.Pointer makes the pointer swap race-free, NOT the pointee *T.
// AtomicCapsule[T] enforces linear pointee ownership:
//   - AtomicStoreMove / AtomicSwapMove consume Live[R] -> Moved[R] (O5.2 move-freeze).
//   - Take atomically swaps the pointer with nil, transferring exclusive ownership
//     of *T to the caller without concurrent pointee readers/writers.
type AtomicCapsule[T any] struct {
	ptr atomic.Pointer[T]
}

func NewAtomicCapsule[R any, T any](_ Live[R], initial *T) (*AtomicCapsule[T], Moved[R]) {
	c := &AtomicCapsule[T]{}
	c.ptr.Store(initial)
	return c, Moved[R]{}
}

func AtomicStoreMove[R any, T any](c *AtomicCapsule[T], _ Live[R], next *T) Moved[R] {
	c.ptr.Store(next)
	return Moved[R]{}
}

func AtomicSwapMove[R any, T any](c *AtomicCapsule[T], _ Live[R], next *T) (*T, Moved[R]) {
	prev := c.ptr.Swap(next)
	return prev, Moved[R]{}
}

func (c *AtomicCapsule[T]) Take() *T {
	return c.ptr.Swap(nil)
}

// -----------------------------------------------------------------------------
// 2. LinearPool[T] — Flips GUARANTEES Row 10 (sync.Pool)
// -----------------------------------------------------------------------------
// sync.Pool is unsafe in unverified Go when a caller retains an alias after
// pool.Put(x). LinearPool[T] consumes Live[R] -> Moved[R] on PoolPutMove and
// runs a mandatory reset hook before returning the object to sync.Pool.
type LinearPool[T any] struct {
	pool  sync.Pool
	reset func(*T)
}

func NewLinearPool[T any](alloc func() *T, reset func(*T)) *LinearPool[T] {
	return &LinearPool[T]{
		pool: sync.Pool{
			New: func() any { return alloc() },
		},
		reset: reset,
	}
}

func (p *LinearPool[T]) Get() *T {
	return p.pool.Get().(*T)
}

func PoolPutMove[R any, T any](p *LinearPool[T], _ Live[R], item *T) Moved[R] {
	if p.reset != nil && item != nil {
		p.reset(item)
	}
	p.pool.Put(item)
	return Moved[R]{}
}

// PoolPutMoveUnreset demonstrates Falsifier P4a: consuming the outer root R
// stops caller use-after-Put, but without resetting interior reference fields,
// stale sub-slice/map contents cross pool epochs across tasks.
func PoolPutMoveUnreset[R any, T any](p *LinearPool[T], _ Live[R], item *T) Moved[R] {
	p.pool.Put(item)
	return Moved[R]{}
}

// -----------------------------------------------------------------------------
// 3. MutexCapsule[T] — Flips GUARANTEES Row 8 (managed("reason", "mutex"))
// -----------------------------------------------------------------------------
// Replaces unchecked `managed("...", "mutex")` string blocks with a checked
// lexical borrow window `WithLock(func(ScopedLoan[T]))`. Under O2/O6, a
// ScopedLoan[T] is a non-sendable borrow handle (cannot cross `task` or `<-` or
// be stored into an escaping struct/closure).
type ScopedLoan[T any] struct {
	val *T
}

func (l ScopedLoan[T]) Get() *T { return l.val }

type MutexCapsule[T any] struct {
	mu  sync.Mutex
	val T
}

func NewMutexCapsule[R any, T any](_ Live[R], initial T) (*MutexCapsule[T], Moved[R]) {
	return &MutexCapsule[T]{val: initial}, Moved[R]{}
}

func (m *MutexCapsule[T]) WithLock(fn func(loan ScopedLoan[T])) {
	m.mu.Lock()
	defer m.mu.Unlock()
	fn(ScopedLoan[T]{val: &m.val})
}
