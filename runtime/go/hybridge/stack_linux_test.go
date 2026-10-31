package hybridge

import (
	"runtime/debug"
	"sync/atomic"
	"syscall"
	"testing"
	"unsafe"
)

func TestLinuxDeepStackPool(t *testing.T) {
	pageSize := syscall.Getpagesize()
	if pageSize <= 0 {
		t.Fatalf("syscall.Getpagesize() = %d, want > 0", pageSize)
	}
	seen := make(map[int]bool)
	for range deepStacks {
		i := acquireDeep()
		defer releaseDeep(i)
		if seen[i] {
			t.Fatalf("stack %d acquired twice before release", i)
		}
		seen[i] = true
		top := deepTops[i]
		if top%uintptr(pageSize) != 0 {
			t.Fatalf("stack %d top %#x is not native-page (%d) aligned", i, top, pageSize)
		}
		mapping := deepMappings[i]
		if len(mapping) < deepStackSz+pageSize {
			t.Fatalf("stack %d mapping len = %d, want >= %d", i, len(mapping), deepStackSz+pageSize)
		}
		if top != uintptr(unsafe.Pointer(&mapping[len(mapping)-1]))+1 {
			t.Fatal("stack top does not match the mapping end")
		}
		// Both ends of the usable region must be writable.
		mapping[pageSize] = byte(i)
		mapping[len(mapping)-1] = byte(i)
	}
}

func TestLinuxDeepStackGuard(t *testing.T) {
	i := acquireDeep()
	defer releaseDeep(i)
	old := debug.SetPanicOnFault(true)
	defer debug.SetPanicOnFault(old)
	defer func() {
		if recover() == nil {
			t.Error("write below the usable stack did not fault: guard is unprotected")
		}
	}()
	deepMappings[i][syscall.Getpagesize()-1] = 1
}

func TestReleaseDeepOutOfBoundsNoOp(t *testing.T) {
	i := acquireDeep()
	defer releaseDeep(i)
	before := atomic.LoadUint64(&deepBits)
	releaseDeep(-1)
	releaseDeep(deepStacks)
	releaseDeep(deepStacks + 100)
	if after := atomic.LoadUint64(&deepBits); after != before {
		t.Fatalf("out-of-range releaseDeep mutated deepBits: got %#x, want %#x", after, before)
	}
}
