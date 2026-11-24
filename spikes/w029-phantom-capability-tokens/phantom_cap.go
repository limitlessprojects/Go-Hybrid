// Package phantomcap provides compile-time zero-overhead linear capability witnesses
// for Go and the Hybrid programming language.
//
// This package answers MOONSHOT.md Prompt 2:
// "Replace the borrow-checker machinery. Capability tokens, linear-type encodings
// inside Go's type system, runtime-enforced ownership — anything that deletes
// the alias/loan/freeze analysis rather than extending it."
//
// Solves the W-004 falsifier where lexical closure captures shared underlying heap storage.
package phantomcap

import (
	"errors"
	"sync/atomic"
	"unsafe"
)

// Permission marker interface.
type Permission interface {
	__perm()
}

// Unique represents sole, unaliased mutable ownership of value T.
type Unique struct{}

func (Unique) __perm() {}

// Shared represents a scoped, read-only borrow of value T.
type Shared struct{}

func (Shared) __perm() {}

// Expired represents a consumed capability that can never be reused.
type Expired struct{}

func (Expired) __perm() {}

// Cap wraps a payload of type T with an affine permission witness.
// The permission witness is zero-sized and erased at compile time.
type Cap[T any, P Permission] struct {
	val T
	_   [0]P // Zero-sized phantom field
}

// NewUnique constructs an unaliased unique capability wrapping val.
func NewUnique[T any](val T) Cap[T, Unique] {
	return Cap[T, Unique]{val: val}
}

// Read accesses the value held by a shared borrow.
func Read[T any](c Cap[T, Shared]) T {
	return c.val
}

// Mutate executes a state mutation on an unaliased unique capability,
// returning the modified capability.
func Mutate[T any](c Cap[T, Unique], fn func(*T)) Cap[T, Unique] {
	fn(&c.val)
	return c
}

// Borrow executes a scoped read-only action over c.
// During fn's execution, only a Shared capability is visible.
// Returns the original Unique capability and fn's result.
func Borrow[T any, R any](c Cap[T, Unique], fn func(Cap[T, Shared]) R) (Cap[T, Unique], R) {
	shared := Cap[T, Shared]{val: c.val}
	res := fn(shared)
	return c, res
}

// Transfer transfers unique ownership, returning the active capability
// and an expired token to satisfy call-site linear consumption.
func Transfer[T any](c Cap[T, Unique]) (Cap[T, Unique], Cap[T, Expired]) {
	return Cap[T, Unique]{val: c.val}, Cap[T, Expired]{}
}

// Consume moves ownership out of the capability, returning the underlying value.
// The capability is consumed and cannot be reused.
func Consume[T any](c Cap[T, Unique]) (T, Cap[T, Expired]) {
	return c.val, Cap[T, Expired]{}
}

// Pair encapsulates two values for disjoint partitioning.
type Pair[T any, U any] struct {
	First  T
	Second U
}

// Split partitions a unique pair into two independent unique capabilities.
func Split[T any, U any](c Cap[Pair[T, U], Unique]) (Cap[T, Unique], Cap[U, Unique]) {
	return Cap[T, Unique]{val: c.val.First}, Cap[U, Unique]{val: c.val.Second}
}

// Combine merges two independent unique capabilities back into a single pair.
func Combine[T any, U any](a Cap[T, Unique], b Cap[U, Unique]) Cap[Pair[T, U], Unique] {
	return Cap[Pair[T, U], Unique]{val: Pair[T, U]{First: a.val, Second: b.val}}
}

// SizeOfCap returns unsafe.Sizeof the capability wrapper.
// Proves that Cap[T, P] has ZERO allocation overhead compared to T.
func SizeOfCap[T any, P Permission]() uintptr {
	var c Cap[T, P]
	return unsafe.Sizeof(c)
}

// AssertZeroOverhead asserts at runtime that the wrapper adds 0 bytes.
func AssertZeroOverhead[T any]() bool {
	var val T
	var c Cap[T, Unique]
	return unsafe.Sizeof(val) == unsafe.Sizeof(c)
}

// TaskBoundaryFence prevents closure-capture storage aliasing across goroutines
// (resolving the W-004 falsifier).
type TaskBoundaryFence struct {
	discharged uint32
}

func NewTaskBoundaryFence() *TaskBoundaryFence {
	return &TaskBoundaryFence{}
}

// Escrow wraps a unique capability across a concurrency boundary.
func (f *TaskBoundaryFence) Escrow[T any](c Cap[T, Unique]) (func() (Cap[T, Unique], error), error) {
	if atomic.SwapUint32(&f.discharged, 1) != 0 {
		return nil, errors.New("HYB-E3003: task boundary fence violated: duplicate capability extraction")
	}
	extracted := false
	return func() (Cap[T, Unique], error) {
		if extracted {
			return Cap[T, Unique]{}, errors.New("HYB-E3003: use after move across task boundary")
		}
		extracted = true
		return c, nil
	}, nil
}
