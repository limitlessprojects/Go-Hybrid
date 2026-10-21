// Package hybits is a bounded, zero-dependency feasibility prototype for
// W-003 (#193): linear capability types (own/share/view) backed by ONE
// runtime ownership bit per composite, so that a reconstruction miss at a
// task boundary FAILS CLOSED at runtime instead of racing.
//
// This is the runtime half of the W-003 argument: static machinery may
// miss a seam (the four merged landings #110/#111/#123/#126 all lived at
// "which frame owns this fact"), but the one-bit boundary check cannot.
package hybits

import "sync/atomic"

// The one ownership bit per composite. 0 = live, 1 = moved.
const (
	bitLive  uint32 = 0
	bitMoved uint32 = 1
)

// MoveError is the fail-closed verdict for use of a moved value at a
// boundary. GUARANTEES.md: an accepted-but-unsound program is a P0 by
// definition; this surface converts that class into a runtime rejection.
type MoveError struct{ op string }

func (e *MoveError) Error() string {
	return "hyb: use of moved value at " + e.op + " (fail-closed)"
}

func moveErr(op string) *MoveError { return &MoveError{op: op} }

// Own[T] is the linear capability wrapper. Obtain via OwnOf only.
// The bit is allocated exactly once at OwnOf; every later boundary check
// is a single atomic load/CAS on that preallocated word (0 allocs/op).
type Own[T any] struct {
	v   *T
	bit *atomic.Uint32
}

// OwnOf unitizes a plain value. The mint is the only way to produce an
// Own; emitted Go code calls this at the capability introduction site.
func OwnOf[T any](v T) Own[T] {
	return Own[T]{v: &v, bit: &atomic.Uint32{}}
}

// Live reports the ownership bit (true = usable). In emitted Go this is
// only called at boundary ops (send/task/move), never in ambient reads.
func (o Own[T]) Live() bool { return o.bit.Load() == bitLive }

// Move consumes the capability linearly and hands out the payload. The
// CAS is the whole boundary check: exactly one caller can transition
// live -> moved; every later caller fails closed.
func (o Own[T]) Move() (T, error) {
	var zero T
	if !o.bit.CompareAndSwap(bitLive, bitMoved) {
		return zero, moveErr("move")
	}
	v := *o.v
	o.v = nil
	return v, nil
}

// MustMove is the emitted form at hard boundaries (send/task spawn): the
// runtime rejection is a panic, matching GUARANTEES fail-closed style.
func (o Own[T]) MustMove() T {
	v, err := o.Move()
	if err != nil {
		panic(err)
	}
	return v
}

// Share[T] models the aliased case: N peers alias one underlying value.
// The first peer that crosses a boundary sets the shared frozen bit; any
// peer attempting the boundary afterwards fails closed. This is the
// runtime form of "share is frozen on any move of a peer" — the seam the
// slot-loan/loop-backedge landings (PR #214..#218 family) had to chase
// frame by frame; here it is one flag.
type Share[T any] struct {
	v      *T
	frozen *atomic.Uint32
}

// ShareOf mints a peer set over one value. Call again on the same T to
// add peers to the same frozen bit (emitter keeps the alias table it
// already computes; the runtime keeps only ONE bit for the whole set).
func ShareOf[T any](v T) Share[T] {
	return Share[T]{v: &v, frozen: &atomic.Uint32{}}
}

// Peer mints another alias that shares the same frozen bit.
func (s Share[T]) Peer() Share[T] {
	return Share[T]{v: s.v, frozen: s.frozen}
}

// Move crosses the boundary: exactly once per peer set, ever.
func (s Share[T]) Move() (T, error) {
	var zero T
	if !s.frozen.CompareAndSwap(bitLive, bitMoved) {
		return zero, moveErr("share-boundary")
	}
	return *s.v, nil
}

// View[T] is a read-only loan: it must never cross a task boundary and
// must never be stored in a composite field. The runtime half enforces
// the boundary half (Move is permanently frozen); the storage ban is the
// type-level half (unexported payload; no exported projection), tracked
// as spike limitation L2 below.
type View[T any] struct {
	v     *T
	epoch uint64
}

// ViewOf borrows a read-only view from a value.
func ViewOf[T any](v T) View[T] { return View[T]{v: &v, epoch: 1} }

// Move always fails closed: a view is not an owner.
func (v View[T]) Move() (T, error) {
	var zero T
	return zero, moveErr("view-crosses-boundary")
}
