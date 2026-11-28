// Package gencap prototypes the runtime half of W-003 (#193): a mark that
// lives on the STORAGE (one generation word in a shared header) instead of
// on the binding. Every syntactic use of an own[T] binding lowers to Get();
// `move x` lowers to Move(). A stale capability - captured by a closure,
// copied into a missed alias, or held by a defer - fails closed on its next
// use, whichever binding it came from.
package gencap

import "sync/atomic"

// ErrMoved is the panic value of a stale use (fail closed, before any access
// to the storage).
type ErrMoved struct{ Have, Live uint32 }

func (e ErrMoved) Error() string { return "hyb: use of moved value (fail-closed)" }

type header[T any] struct {
	gen atomic.Uint32
	v   T
}

// Own is the generation-checked capability (variant G).
type Own[T any] struct {
	h   *header[T]
	gen uint32
}

// Mint wraps a fresh composite. The prototype allocates one header per mint;
// the header-embedded variant (no wrapper allocation) is out of scope.
func Mint[T any](v T) Own[T] {
	h := &header[T]{v: v}
	h.gen.Store(1)
	return Own[T]{h: h, gen: 1}
}

// Get is the per-use check: one atomic load and one compare.
func (o Own[T]) Get() T {
	if live := o.h.gen.Load(); live != o.gen {
		panic(ErrMoved{Have: o.gen, Live: live})
	}
	return o.h.v
}

// Move consumes o: CAS gen -> gen+1, return the only live capability.
func (o Own[T]) Move() Own[T] {
	next := o.gen + 1
	if !o.h.gen.CompareAndSwap(o.gen, next) {
		panic(ErrMoved{Have: o.gen, Live: o.h.gen.Load()})
	}
	return Own[T]{h: o.h, gen: next}
}

// Bit is variant B1: the literal "one bit" of #193 (generation mod 2).
type Bit[T any] struct {
	h   *header[T]
	gen uint32
}

func MintBit[T any](v T) Bit[T] {
	h := &header[T]{v: v}
	return Bit[T]{h: h, gen: 0}
}

func (o Bit[T]) Get() T {
	if live := o.h.gen.Load(); live != o.gen {
		panic(ErrMoved{Have: o.gen, Live: live})
	}
	return o.h.v
}

func (o Bit[T]) Move() Bit[T] {
	next := (o.gen + 1) & 1
	if !o.h.gen.CompareAndSwap(o.gen, next) {
		panic(ErrMoved{Have: o.gen, Live: o.h.gen.Load()})
	}
	return Bit[T]{h: o.h, gen: next}
}

// hyTake mirrors the W-017 fast-path lowering from #370 (control M1):
// zero the source binding, hand the value over.
func hyTake[T any](p *T) T {
	v := *p
	var zero T
	*p = zero
	return v
}
