package take_detach_test

import (
	"maps"
	"runtime"
	"sync"
	"testing"
)

// __hy_take implements the M1 fast-path operational move lowering for
// `move <place>` when `may_share` is empty: returns the value and zeroes the
// source binding in place (0 allocations, 2 pointer/word stores).
//
//go:noinline
func hyTakeNoInline[T any](src *T) T {
	v := *src
	var zero T
	*src = zero
	return v
}

// Inlineable version emitted in generated Go code (`cmd/compile` inlines this
// to a register copy + stack/field zero).
func __hy_take[T any](src *T) T {
	v := *src
	var zero T
	*src = zero
	return v
}

// __hy_clip_slice clips capacity (`s[:len(s):len(s)]`) on slice moves so an
// `append` in the receiving task allocates a fresh backing array instead of
// overwriting elements beyond `len(s)` in a shared backing array.
func __hy_clip_slice[T any](s []T) []T {
	if s == nil {
		return nil
	}
	return s[:len(s):len(s)]
}

// __hy_detach_map detaches a map header when `may_share[src]` is non-empty at
// the `move` boundary (e.g. an earlier struct/call/closure alias still holds a
// reference to the same map header).
func __hy_detach_map[K comparable, V any](m map[K]V) map[K]V {
	if m == nil {
		return nil
	}
	return maps.Clone(m)
}

// __hy_detach_slice detaches a slice backing array when `may_share[src]` is
// non-empty and elements may be mutated in place across tasks.
func __hy_detach_slice[T any](s []T) []T {
	if s == nil {
		return nil
	}
	out := make([]T, len(s))
	copy(out, s)
	return out
}

type Holder struct {
	M map[string]int
	S []int
}

func __hy_detach_Holder(h Holder) Holder {
	return Holder{
		M: __hy_detach_map(h.M),
		S: __hy_detach_slice(h.S),
	}
}

// Simulated hyrt generation-checked handle table (mirrors runtime/rust +
// runtime/go/hyrt: handle 0 is the invalid sentinel).
type RustHandle uint64

const InvalidHandle RustHandle = 0

var sinkMap map[string]int
var sinkSlice []int
var sinkHandle RustHandle

// M0: Baseline — current hy-emit no-op lowering (`t := m`).
func BenchmarkM0_NoopMove(b *testing.B) {
	m := map[string]int{"a": 1, "b": 2}
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		src := m
		t := src // current hy-emit: Expr::Move => self.emit_expr(operand)
		sinkMap = t
	}
}

// M1: Operational take fast-path (`__hy_take(&src)`), inlineable.
func BenchmarkM1_HyTake_FastPath_Inline(b *testing.B) {
	m := map[string]int{"a": 1, "b": 2}
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		src := m
		t := __hy_take(&src)
		sinkMap = t
	}
}

// M1b: Operational take fast-path (`hyTakeNoInline(&src)`), call-boundary floor.
func BenchmarkM1b_HyTake_FastPath_NoInline(b *testing.B) {
	m := map[string]int{"a": 1, "b": 2}
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		src := m
		t := hyTakeNoInline(&src)
		sinkMap = t
	}
}

// M2: Operational take + 3-index slice capacity clip (`s[:len(s):len(s)]`).
func BenchmarkM2_HyTake_ClippedSlice(b *testing.B) {
	base := []int{1, 2, 3, 4, 5, 6, 7, 8}
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		src := base[:4]
		t := __hy_clip_slice(__hy_take(&src))
		sinkSlice = t
	}
}

// M3: Selective header detach (`__hy_detach_map(__hy_take(&src))`) when
// `may_share` is non-empty (8-entry map).
func BenchmarkM3_HyDetach_SharedMap8(b *testing.B) {
	m := map[string]int{
		"k0": 0, "k1": 1, "k2": 2, "k3": 3,
		"k4": 4, "k5": 5, "k6": 6, "k7": 7,
	}
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		src := m
		t := __hy_detach_map(__hy_take(&src))
		sinkMap = t
	}
}

// M4: Rust own-handle operational move (`__hy_take(&h)` zeros uint64 handle to
// the 0 invalid sentinel in hyrt, preserving the CGo/Rust staticlib bridge).
func BenchmarkM4_HyTake_RustOwnHandle(b *testing.B) {
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		h := RustHandle(0x0000_0001_0000_002a)
		moved := __hy_take(&h)
		sinkHandle = moved
	}
}

// TestChain1_CallAndMethodReturnSharing verifies that the #38/#105/#120 report
// chain (a helper/method returning input-backed storage that is moved to a
// spawned task while the caller mutates the original) cannot data-race under
// `go test -race` when `move` lowers to `__hy_detach_Holder(__hy_take(&h))`.
func TestChain1_CallAndMethodReturnSharing_RaceFree(t *testing.T) {
	runtime.GOMAXPROCS(4)
	for iter := 0; iter < 500; iter++ {
		orig := map[string]int{"k": 1}
		// Helper returns a struct wrapping `orig` (the #38/#120 shape).
		wrap := func(in map[string]int) Holder {
			return Holder{M: in, S: []int{10, 20}}
		}
		h := wrap(orig)

		var wg sync.WaitGroup
		wg.Add(1)
		// Lowering of `task worker(move h)` on shared-peer path:
		movedH := __hy_detach_Holder(__hy_take(&h))
		if h.M != nil || h.S != nil {
			t.Fatalf("expected source binding h to be zeroed by __hy_take")
		}
		go func(w Holder) {
			defer wg.Done()
			w.M["k"] = 99
			w.S[0] = 99
		}(movedH)

		// Caller concurrently mutates `orig`.
		orig["k"] = 2
		wg.Wait()
	}
}

// TestChain2_ClosureAndRebindLaundering verifies that the #75/#137/#206 report
// chain (closure capture / self-rebind / peer-rebind followed by task move)
// cannot race under `go test -race`.
func TestChain2_ClosureAndRebindLaundering_RaceFree(t *testing.T) {
	runtime.GOMAXPROCS(4)
	for iter := 0; iter < 500; iter++ {
		m := map[string]int{"x": 0}
		// Closure captures `m` and launders it into `p2` (#75 / #137 / #206).
		launder := func() Holder { return Holder{M: m} }
		p2 := launder()
		var h Holder
		// Lowering of `h = move p2` followed by `task worker(move h)`:
		h = __hy_take(&p2)
		if p2.M != nil {
			t.Fatalf("expected p2.M to be zeroed after move")
		}

		var wg sync.WaitGroup
		wg.Add(1)
		moved := __hy_detach_Holder(__hy_take(&h))
		go func(w Holder) {
			defer wg.Done()
			w.M["x"] = 42
		}(moved)

		m["x"] = 7
		wg.Wait()
	}
}

// TestChain3_SubsliceAppendCapacityClip verifies that moving a sub-slice with
// `__hy_clip_slice(__hy_take(&sub))` prevents `append` in the spawned task from
// racing on the caller's spare backing-array capacity.
func TestChain3_SubsliceAppendCapacityClip_RaceFree(t *testing.T) {
	runtime.GOMAXPROCS(4)
	for iter := 0; iter < 500; iter++ {
		buf := make([]int, 4, 8)
		buf[0], buf[1], buf[2], buf[3] = 1, 2, 3, 4
		sub := buf[:2]

		var wg sync.WaitGroup
		wg.Add(1)
		movedSub := __hy_clip_slice(__hy_take(&sub))
		if sub != nil {
			t.Fatalf("expected sub to be nil after __hy_take")
		}
		go func(s []int) {
			defer wg.Done()
			// Without 3-index capacity clipping, append(s, 99) overwrites buf[2]
			// and races with the caller reading/writing buf[2:4].
			s = append(s, 99)
			if s[2] != 99 {
				t.Errorf("unexpected append value: %d", s[2])
			}
		}(movedSub)

		// Caller concurrently accesses the remaining slice tail buf[2:4].
		buf[2] = 30
		if buf[2] != 30 {
			t.Fatalf("backing array corrupted by spawned task")
		}
		wg.Wait()
	}
}

// TestRustOwnHandle_TakeZerosToInvalidSentinel verifies that `__hy_take(&h)`
// zeroes a Rust `own handle` (`uint64`) to `0` (`InvalidHandle` in hyrt) so
// any post-move access on the Go side deterministically sees the invalid
// sentinel while the moved handle remains valid in the receiving task.
func TestRustOwnHandle_TakeZerosToInvalidSentinel(t *testing.T) {
	h := RustHandle(0x0000_0001_0000_0007)
	transferred := __hy_take(&h)
	if h != InvalidHandle {
		t.Fatalf("expected moved-from Rust handle to be 0 (InvalidHandle), got %#x", h)
	}
	if transferred != RustHandle(0x0000_0001_0000_0007) {
		t.Fatalf("expected transferred handle to preserve generation+slot, got %#x", transferred)
	}
}
