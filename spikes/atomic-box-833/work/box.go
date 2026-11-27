// Package boxspike investigates #833. It is not a checked Hybrid runtime API.
package boxspike

import "sync/atomic"

// Box must not be copied after first use. The caller must exclusively own the
// entire graph of next, relinquish it on success, and dereference expected only
// after acquiring it. Go cannot enforce those preconditions for raw pointers.
// The checks below catch direct self-exchange, not arbitrary interior aliases.
type Box[T any] struct {
	p atomic.Pointer[T]
}

func (b *Box[T]) Swap(next *T) *T {
	old := b.p.Swap(next)
	if next != nil && old == next {
		panic("self-exchange cannot acquire an object still in the box")
	}
	return old
}

// CompareAndSwap leaves next with its caller on failure. expected is only an
// identity, not permission to read its fields. There is deliberately no Load.
func (b *Box[T]) CompareAndSwap(expected, next *T) (bool, *T) {
	if expected != nil && expected == next {
		panic("self-exchange cannot acquire an object still in the box")
	}
	if !b.p.CompareAndSwap(expected, next) {
		return false, nil
	}
	return true, expected
}
