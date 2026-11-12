package hybridge

// Pooled guarded Rust stacks (Spike 3): the deep-path crossing runs Rust on a
// 1 MiB mmap'd stack with a native-size guard page, OUTSIDE the Go heap (never
// scanned by the GC). Slots are exclusive per crossing via a 64-bit atomic
// bitmap — the fix for the shared-stack collision that Spike 1's single
// mmap stack exhibited under concurrent cleanups (two goroutines on different
// Ms crossing at the same instant overwrite each other's Rust frames).
//
// The fast path does not use these stacks at all: shallow callees run on the
// caller's own goroutine stack after a stackguard headroom check.

import (
	"math/bits"
	"runtime"
	"sync/atomic"
	"syscall"
	"unsafe"
)

const (
	deepStacks  = 64      // pool size (one bit each)
	deepStackSz = 1 << 20 // 1 MiB per stack
)

var (
	deepMappings [deepStacks][]byte  // retain the guarded mappings for the pool lifetime
	deepTops     [deepStacks]uintptr // stack tops (base + guard + size)
	deepBits     uint64              // bitmap: bit i set = stack i in use
)

func init() {
	// Apple Silicon uses 16 KiB pages; Intel Macs use 4 KiB pages.
	pageSize := syscall.Getpagesize()
	for i := 0; i < deepStacks; i++ {
		total := deepStackSz + pageSize
		b, err := syscall.Mmap(-1, 0, total, syscall.PROT_READ|syscall.PROT_WRITE, syscall.MAP_PRIVATE|syscall.MAP_ANON)
		if err != nil {
			panic("hybridge: mmap deep stack: " + err.Error())
		}
		// guard page at the bottom (stack grows down toward it)
		if err := syscall.Mprotect(b[:pageSize], syscall.PROT_NONE); err != nil {
			_ = syscall.Munmap(b)
			panic("hybridge: mprotect guard page: " + err.Error())
		}
		deepMappings[i] = b
		deepTops[i] = uintptr(unsafe.Pointer(&b[0])) + uintptr(total)
	}
}

// acquireDeep returns the index of an exclusively-held pooled stack.
func acquireDeep() int {
	for {
		old := atomic.LoadUint64(&deepBits)
		if old == ^uint64(0) {
			runtime.Gosched() // pool exhausted (64 concurrent deep calls): back off
			continue
		}
		i := bits.TrailingZeros64(^old)
		b := uint64(1) << uint(i)
		if atomic.CompareAndSwapUint64(&deepBits, old, old|b) {
			return i
		}
	}
}

// releaseDeep frees a pooled stack acquired by acquireDeep.
func releaseDeep(i int) {
	if i < 0 || i >= deepStacks {
		return
	}
	atomic.AndUint64(&deepBits, ^(uint64(1) << uint(i)))
}
