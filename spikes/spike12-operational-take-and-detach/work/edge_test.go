package take_detach_test

import (
	"os"
	"sync"
	"testing"
)

func gate(t *testing.T) {
	if os.Getenv("HY_NEG") != t.Name() {
		t.Skip("negative control; run with HY_NEG=<TestName> to observe the race detector catch the omitted mechanism")
	}
}

// N1: fast path (`__hy_take` only) when an alias exists that `may_share` missed:
// deterministically triggers `WARNING: DATA RACE` under `go test -race`.
func TestNeg_FastPathMissedAlias(t *testing.T) {
	gate(t)
	m := map[string]int{"k": 1}
	alias := m // missed by may_share
	var wg sync.WaitGroup
	wg.Add(1)
	moved := __hy_take(&m)
	go func(w map[string]int) { defer wg.Done(); w["k"] = 9 }(moved)
	alias["k"] = 2
	wg.Wait()
}

// N2: sub-slice move without `__hy_clip_slice`: `append` in the spawned task
// overwrites `buf[2]` and deterministically races under `go test -race`.
func TestNeg_NoClip(t *testing.T) {
	gate(t)
	buf := make([]int, 4, 8)
	sub := buf[:2]
	var wg sync.WaitGroup
	wg.Add(1)
	moved := __hy_take(&sub)
	go func(s []int) { defer wg.Done(); s = append(s, 99); _ = s }(moved)
	buf[2] = 30
	wg.Wait()
}

// E1: `defer f(m)` evaluates `m` at the `defer` statement (before `move m`), so
// the deferred call retains a live map header that races with the spawned task
// unless `defer` arguments are tracked in `may_share` or treated as loans.
func TestNeg_DeferArgEvaluatedBeforeMove(t *testing.T) {
	gate(t)
	var wg sync.WaitGroup
	func() {
		m := map[string]int{"k": 1}
		defer func(dm map[string]int) { dm["k"] = 3 }(m)
		wg.Add(1)
		moved := __hy_take(&m)
		go func(w map[string]int) { defer wg.Done(); w["k"] = 9 }(moved)
	}()
	wg.Wait()
}

// E2: `__hy_take(&m)` writes to the variable slot `m`, which races if an
// earlier concurrent goroutine captured `m` by reference.
func TestNeg_TakeWritesCapturedSlot(t *testing.T) {
	gate(t)
	m := map[string]int{"k": 1}
	done := make(chan struct{})
	go func() { _ = len(m); close(done) }()
	_ = __hy_take(&m)
	<-done
}

// E3: shallow `maps.Clone` on `map[string][]int` detaches the outer map header
// but still shares inner slice backing arrays unless detach is generated per
// nested reference type shape (`__hy_detach_<Shape>`).
func TestNeg_ShallowDetachMapOfSlices(t *testing.T) {
	gate(t)
	inner := []int{1, 2}
	m := map[string][]int{"k": inner}
	var wg sync.WaitGroup
	wg.Add(1)
	moved := __hy_detach_map(__hy_take(&m))
	go func(w map[string][]int) { defer wg.Done(); w["k"][0] = 9 }(moved)
	inner[0] = 5
	wg.Wait()
}

// E4: composite holding both a Go map and a Rust `own handle` (`uint64`).
type Res struct {
	M map[string]int
	H RustHandle
}

func TestComposite_MapAndHandle(t *testing.T) {
	r := Res{M: map[string]int{"k": 1}, H: 42}
	moved := __hy_take(&r)
	if r.M != nil || r.H != InvalidHandle {
		t.Fatalf("source not fully zeroed: %+v", r)
	}
	copyBefore := moved // `b := a` struct copy duplicates the uint64 handle
	_ = __hy_take(&moved)
	if copyBefore.H != 42 {
		t.Fatal("unexpected")
	}
	// Demonstrates why duplicated own handles must be rejected fail-closed
	// (HYB-E3005) statically and guarded by hyrt's generation check at runtime.
}

// E5: nil map / nil slice / zero-len non-nil slice moves and Go's silent
// zero-value read semantics after `__hy_take`.
func TestNilAndEmptyMoves(t *testing.T) {
	var m map[string]int
	var s []int
	if __hy_detach_map(__hy_take(&m)) != nil || __hy_clip_slice(__hy_take(&s)) != nil {
		t.Fatal("nil must stay nil")
	}
	buf := make([]int, 0, 8)
	e := __hy_clip_slice(__hy_take(&buf))
	if e == nil || cap(e) != 0 {
		t.Fatalf("empty slice must clip to cap 0 and stay non-nil, got cap %d nil=%v", cap(e), e == nil)
	}
	// Post-move reads and slice appends on zeroed Go bindings succeed silently
	// (only a nil-map write panics), which is why static HYB-E3003 remains required.
	if len(m) != 0 || m["k"] != 0 {
		t.Fatal("unexpected")
	}
	s = append(s, 1)
	defer func() {
		if recover() == nil {
			t.Fatal("expected panic writing moved-from nil map")
		}
	}()
	m["k"] = 1
}
