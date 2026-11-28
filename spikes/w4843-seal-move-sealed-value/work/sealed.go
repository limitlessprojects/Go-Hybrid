// Package w4843sealmove is the M1 feasibility spike for Wildcard #4843:
// `seal move x` -> `sealed[T]` (Rule O5.7 / Sendable Class 4b).
//
// It models the runtime side of the proposal: a zero-copy, read-only
// projection of a dynamically-built map/slice/struct that a sender can keep
// reading and that broadcasts across goroutines without cloning. The static
// admission rule (ownership, aliasing, and interior-sealability checks that a
// real Tier-0 checker would enforce before emitting this runtime shape) is
// modeled separately in checker/seal_checker.py — this package only proves
// the runtime representation and the two declared Go-runtime falsifiers that
// a runtime-only implementation must still defend against (F1: slice spare
// capacity; F3: interior closures).
package w4843sealmove

import (
	"errors"
	"reflect"
)

// ErrNotSealable is HYB-E3015: the value's representation recursively
// contains a closure, channel, interface, or pointer and cannot be sealed.
var ErrNotSealable = errors.New("HYB-E3015: type is not sealable (interior closure/chan/interface/ptr)")

// SealedSlice is the `sealed[[]T]` projection: Copy, read-only, no index
// assignment and no append — only value-copy reads (R1/R3 surface from the
// issue's "Read-Only Projection Surface" section).
type SealedSlice[T any] struct {
	data []T
}

// SealSlice consumes exclusive ownership of s and returns its sealed, O(1),
// zero-allocation projection. Remedy R1: the backing array is clipped to
// cap==len so that no append through any other alias of the pre-seal slice
// can write into spare capacity shared with this witness (declared falsifier
// F1 below).
func SealSlice[T any](s []T) SealedSlice[T] {
	return SealedSlice[T]{data: s[:len(s):len(s)]}
}

func (s SealedSlice[T]) Len() int { return len(s.data) }
func (s SealedSlice[T]) At(i int) T { return s.data[i] }

func (s SealedSlice[T]) Range(fn func(i int, v T) bool) {
	for i, v := range s.data {
		if !fn(i, v) {
			return
		}
	}
}

// naiveSealedSlice is the F1 falsifier's counter-example: it wraps the slice
// header as-is, without clipping capacity, which is the shape a naive
// "seal == read-only view" implementation would produce.
type naiveSealedSlice[T any] struct {
	data []T
}

func naiveSealSlice[T any](s []T) naiveSealedSlice[T] {
	return naiveSealedSlice[T]{data: s}
}

func (s naiveSealedSlice[T]) Len() int   { return len(s.data) }
func (s naiveSealedSlice[T]) At(i int) T { return s.data[i] }

// SealedMap is the `sealed[map[K]V]` projection: Copy, read-only, no
// index-assignment, no delete.
type SealedMap[K comparable, V any] struct {
	data map[K]V
}

// SealMap consumes exclusive ownership of m and returns its sealed, O(1),
// zero-allocation projection (no copy of the underlying hmap is made).
func SealMap[K comparable, V any](m map[K]V) SealedMap[K, V] {
	return SealedMap[K, V]{data: m}
}

func (s SealedMap[K, V]) Len() int { return len(s.data) }

func (s SealedMap[K, V]) Get(k K) (V, bool) {
	v, ok := s.data[k]
	return v, ok
}

func (s SealedMap[K, V]) Range(fn func(k K, v V) bool) {
	for k, v := range s.data {
		if !fn(k, v) {
			return
		}
	}
}

// IsSealableType implements R3's is_sealable(T) recursive representation
// check: interior closures, channels, interfaces, and pointers (foreign
// opaque handles, `managed`-tainted bindings) make a type unsealable because
// the checker cannot prove nothing reachable from it can still write.
func IsSealableType(t reflect.Type) bool {
	switch t.Kind() {
	case reflect.Func, reflect.Chan, reflect.Interface, reflect.UnsafePointer, reflect.Ptr:
		return false
	case reflect.Struct:
		for i := 0; i < t.NumField(); i++ {
			if !IsSealableType(t.Field(i).Type) {
				return false
			}
		}
		return true
	case reflect.Slice, reflect.Array:
		return IsSealableType(t.Elem())
	case reflect.Map:
		return IsSealableType(t.Key()) && IsSealableType(t.Elem())
	default:
		return true
	}
}

// SealValue consumes v and returns its sealed projection, or ErrNotSealable
// (HYB-E3015) if v's type recursively contains a non-sealable interior —
// the runtime-side half of declared falsifier F3.
func SealValue[T any](v T) (T, error) {
	if !IsSealableType(reflect.TypeOf(v)) {
		var zero T
		return zero, ErrNotSealable
	}
	return v, nil
}
