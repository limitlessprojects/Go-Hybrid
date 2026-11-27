// Package wseal implements the bounded M1/M2 executable feasibility spike for
// Monotonic Ownership-to-Value Sealing (`seal move x -> sealed[T]`, O5.7 / Class 4b).
//
// It shares the zero-size `Live` / `Moved` phantom witness encoding from W-004
// (#507) and W-022 (#545), enforces 3-index slice capacity clipping (`s[:n:n]`,
// Remedy R1, converging with W-015 #379), and includes minimal harnesses for
// Declared Falsifiers F1 (spare-capacity append clobber), F2 (surviving pre-seal
// interior `may_share` alias), and F3 (stateful interior closure laundering).
package wseal

// Live and Moved are zero-size typestate witnesses carried by owned handles.
type Live struct{}
type Moved struct{}

// OwnMap is a uniquely owned mutable Go map parameterized by region R and
// typestate State (`Live` or `Moved`).
type OwnMap[R any, K comparable, V any, State any] struct {
	data map[K]V
}

// SealedMap is a Sendable Class-4b immutable projection produced by `SealMap`.
// Its backing map field is unexported and exposes zero mutation methods, so it
// may be copied across arbitrary tasks, channels, struct fields, and foreign
// callbacks (`http.Handler`) with 0 allocations and 0 locks.
type SealedMap[K comparable, V any] struct {
	data map[K]V
}

// NewOwnMap allocates a fresh uniquely owned map in the `Live` state.
func NewOwnMap[R any, K comparable, V any](hint int) OwnMap[R, K, V, Live] {
	return OwnMap[R, K, V, Live]{data: make(map[K]V, hint)}
}

// PutMap mutates a `Live` owned map in place. Calling PutMap on a `Moved`
// handle is a compile-time Go type error.
func PutMap[R any, K comparable, V any](m *OwnMap[R, K, V, Live], k K, v V) {
	m.data[k] = v
}

// SealMap performs the irreversible O5.7 transition `OwnMap[R,K,V,Live] ->
// (OwnMap[R,K,V,Moved], SealedMap[K,V])` in O(1) with 0 heap allocations.
func SealMap[R any, K comparable, V any](m OwnMap[R, K, V, Live]) (OwnMap[R, K, V, Moved], SealedMap[K, V]) {
	raw := m.data
	m.data = nil
	return OwnMap[R, K, V, Moved]{}, SealedMap[K, V]{data: raw}
}

// Get reads a key from a sealed map without synchronization overhead.
func (s SealedMap[K, V]) Get(k K) (V, bool) {
	v, ok := s.data[k]
	return v, ok
}

// Len returns the number of entries in the sealed map.
func (s SealedMap[K, V]) Len() int {
	return len(s.data)
}

// ForEach iterates entries in read-only mode until fn returns false.
func (s SealedMap[K, V]) ForEach(fn func(K, V) bool) {
	for k, v := range s.data {
		if !fn(k, v) {
			return
		}
	}
}

// RegisterSealedHandler models GUARANTEES.md Row 9 (`http.HandleFunc`) where
// foreign callbacks require non-const reference captures to be `SealedMap`.
func RegisterSealedHandler[K comparable, V any](
	routes SealedMap[K, V],
	handler func(SealedMap[K, V], K) (V, bool),
) func(K) (V, bool) {
	return func(k K) (V, bool) {
		return handler(routes, k)
	}
}

// OwnSlice is a uniquely owned mutable slice parameterized by region R and State.
type OwnSlice[R any, T any, State any] struct {
	data []T
}

// SealedSlice is a Sendable Class-4b immutable slice projection whose capacity
// is guaranteed equal to its length (`cap == len`, Remedy R1).
type SealedSlice[T any] struct {
	data []T
}

// NewOwnSlice allocates a fresh uniquely owned slice in the `Live` state.
func NewOwnSlice[R any, T any](length, capacity int) OwnSlice[R, T, Live] {
	return OwnSlice[R, T, Live]{data: make([]T, length, capacity)}
}

// SetSlice mutates element `idx` of a `Live` owned slice.
func SetSlice[R any, T any](s *OwnSlice[R, T, Live], idx int, v T) {
	s.data[idx] = v
}

// SealSlice performs the O5.7 transition AND enforces Remedy R1: mandatory
// 3-index capacity clipping `s.data[:n:n]` (`0 B/op`, `0 allocs/op`) so no
// downstream `append` can ever overwrite shared spare backing-array slots.
func SealSlice[R any, T any](s OwnSlice[R, T, Live]) (OwnSlice[R, T, Moved], SealedSlice[T]) {
	n := len(s.data)
	clipped := s.data[:n:n]
	s.data = nil
	return OwnSlice[R, T, Moved]{}, SealedSlice[T]{data: clipped}
}

// At returns the element at index i.
func (s SealedSlice[T]) At(i int) T {
	return s.data[i]
}

// Len returns the slice length.
func (s SealedSlice[T]) Len() int {
	return len(s.data)
}

// Cap returns the slice capacity (always equal to Len() under Remedy R1).
func (s SealedSlice[T]) Cap() int {
	return cap(s.data)
}

// DeriveAppend appends elements to a sealed slice. Because Remedy R1 guarantees
// `cap(s.data) == len(s.data)`, Go's runtime always allocates a fresh backing
// array without mutating `s.data`'s backing storage.
func (s SealedSlice[T]) DeriveAppend(extra ...T) []T {
	return append(s.data, extra...)
}

// ----------------------------------------------------------------------------
// Falsifier Harnesses (F1, F2, F3) — deliberately omit R1 / R2 / R3 to prove
// each pre-registered hazard fires exactly where predicted.
// ----------------------------------------------------------------------------

// NaiveSealedSlice omits Remedy R1 (3-index capacity clip) to test Falsifier F1.
type NaiveSealedSlice[T any] struct {
	raw []T
}

// NaiveUnclippedSealSlice wraps raw without clipping `cap` to `len`.
func NaiveUnclippedSealSlice[T any](raw []T) NaiveSealedSlice[T] {
	return NaiveSealedSlice[T]{raw: raw[:len(raw)]}
}

// DeriveAppendUnsafe runs `append` on an unclipped slice, overwriting spare
// backing-array capacity in place when `cap(s.raw) > len(s.raw)`.
func (s NaiveSealedSlice[T]) DeriveAppendUnsafe(v T) []T {
	return append(s.raw, v)
}

// NewOwnSliceWithPreSealSubslice models Falsifier F2: extracting an interior
// `may_share` sub-slice (`raw[:1]`) prior to `SealSlice` without Remedy R2's
// `storage_component` (`aliases U may_share`) write-freeze.
func NewOwnSliceWithPreSealSubslice[R any, T any](seed []T) (OwnSlice[R, T, Live], []T) {
	buf := make([]T, len(seed))
	copy(buf, seed)
	leakedSub := buf[:1]
	return OwnSlice[R, T, Live]{data: buf}, leakedSub
}

// UncheckedClosureConfig models Falsifier F3: a struct carrying an interior
// stateful closure (`func() int`) that violates representation purity (`R3`).
type UncheckedClosureConfig struct {
	Name   string
	NextID func() int
}

// NaiveSealedStruct wraps any T without Remedy R3's `is_sealable(T)` check.
type NaiveSealedStruct[T any] struct {
	val T
}

// NaiveUncheckedSealStruct wraps v without checking interior representation purity.
func NaiveUncheckedSealStruct[T any](v T) NaiveSealedStruct[T] {
	return NaiveSealedStruct[T]{val: v}
}

// Value returns a shallow copy of the wrapped struct.
func (s NaiveSealedStruct[T]) Value() T {
	return s.val
}
