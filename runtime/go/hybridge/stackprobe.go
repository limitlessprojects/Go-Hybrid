package hybridge

import "unsafe"

// stackProbeSpan: how far below a pooled stack's top StackHighWaterForTest
// paints. Far above any budget the class ledger argues (SmallGuard 1 KiB,
// default guard 4 KiB), so a callee that overshoots its class is MEASURED,
// not clipped.
const stackProbeSpan = 16 << 10

// StackHighWaterForTest runs fn(a1..a4) on a pooled deep stack that was first
// painted with a sentinel, and returns the callee's result plus its stack
// HIGH-WATER MARK in bytes (the deepest byte the callee, and everything it
// called — libc/malloc included — actually wrote below the stack top).
//
// This is the measurement behind interfaces/descriptors.md: a crossing may
// take a guarded fast path ONLY if the callee's real worst-case stack use is
// an upper bound below that guard. Tests only; production code must never
// call it (it paints stackProbeSpan bytes per call).
func StackHighWaterForTest(fn unsafe.Pointer, a1, a2, a3, a4 uintptr) (uint64, uintptr) {
	const sentinel = 0xA5
	i := acquireDeep()
	defer releaseDeep(i)
	top := deepTops[i]
	// The pool is mmap'd outside the Go heap (deepTops keeps only addresses),
	// so the span is rebuilt from the address; unsafe.Add keeps go vet's
	// unsafeptr check quiet for what is a deliberate non-Go-memory pointer.
	mem := unsafe.Slice((*byte)(unsafe.Add(unsafe.Pointer(nil), top-stackProbeSpan)), stackProbeSpan)
	for k := range mem {
		mem[k] = sentinel
	}
	v := callDeep4(fn, a1, a2, a3, a4, top)
	for k := range mem {
		if mem[k] != sentinel {
			return v, stackProbeSpan - uintptr(k)
		}
	}
	return v, 0
}
