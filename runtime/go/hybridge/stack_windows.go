//go:build windows

package hybridge

// Windows twin of stack_linux.go: the SAME pooled guarded Rust stacks, with
// VirtualAlloc/VirtualProtect instead of mmap/mprotect. Pure syscall — no cgo
// — so the hybridge package now COMPILES on windows/amd64 (before this file
// the pool's symbols lived only in stack_linux.go and every Windows build of
// the bridge failed with undefined: acquireDeep).
//
// HONEST LIMIT (community benchmark report, v0.1.0): compiling is not the
// same as the bridge WORKING on Windows. call_amd64.s encodes the SysV amd64
// calling convention; Win64 uses a different one (RCX/RDX/R8/R9 + shadow
// space). The bridge on Windows stays DO-NOT-ADOPT until call_win64.s lands
// and the release matrix's windows leg runs a bridge smoke like arm64's.

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
	pageSize    = 4096    // guard page size

	_wMemCommit     = 0x1000
	_wMemReserve    = 0x2000
	_wPageReadWrite = 0x04
	_wPageNoAccess  = 0x01
)

// kernel32 via NewLazyDLL: resolved at first call, NO cgo, so this builds
// with CGO_ENABLED=0 (the bridge-free contract's toolchain requirement).
var (
	k32                = syscall.NewLazyDLL("kernel32.dll")
	procVirtualAlloc   = k32.NewProc("VirtualAlloc")
	procVirtualProtect = k32.NewProc("VirtualProtect")
)

var (
	deepTops [deepStacks]uintptr // stack tops (base + guard + size)
	deepBits uint64              // bitmap: bit i set = stack i in use
)

func init() {
	for i := 0; i < deepStacks; i++ {
		total := deepStackSz + pageSize
		b, _, err := procVirtualAlloc.Call(
			0, uintptr(total), _wMemReserve|_wMemCommit, _wPageReadWrite)
		if b == 0 {
			panic("hybridge: VirtualAlloc deep stack: " + err.Error())
		}
		// guard page at the bottom (stack grows down toward it)
		var old uint32
		ok, _, err := procVirtualProtect.Call(
			b, uintptr(pageSize), _wPageNoAccess, uintptr(unsafe.Pointer(&old)))
		if ok == 0 {
			panic("hybridge: VirtualProtect guard page: " + err.Error())
		}
		deepTops[i] = b + uintptr(total)
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
