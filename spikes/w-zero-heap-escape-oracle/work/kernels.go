package noheap

var retainedAny any

//go:noinline
func sinkNonLeakingAny(v any) int {
	if x, ok := v.(int); ok {
		return x
	}
	return 0
}

//go:noinline
func sinkLeakingAny(v any) int {
	retainedAny = v
	return 1
}

//go:noinline
func hiddenHeapCallee(n int) int {
	b := make([]byte, n)
	b[0] = 1
	return int(b[0])
}

//line kernels.hy:18
func inlinedXorStep(buf []byte, idx int) byte {
//line kernels.hy:19
	return buf[idx] ^ 0x3c
}

//line kernels.hy:26
func inlinedAppendTag(xs []int, tag int) []int {
//line kernels.hy:28
	return append(xs, tag)
}

//line kernels.hy:34
func K1FixedScratch(xs []int64) int64 {
	var scratch [64]int64
	for i := 0; i < len(xs) && i < len(scratch); i++ {
		scratch[i] = xs[i] * 3
	}
	var sum int64
	for _, v := range scratch {
		sum += v
	}
	return sum
}

//line kernels.hy:48
func K2CallerScratch(dst []byte, src []byte) int {
	n := len(src)
	if len(dst) < n {
		n = len(dst)
	}
	for i := 0; i < n; i++ {
		dst[i] = src[i] ^ 0x5a
	}
	return int(dst[0])
}

//line kernels.hy:61
func K3Const32KiB(seed byte) byte {
	buf := make([]byte, 32*1024)
	buf[0] = seed
	buf[len(buf)-1] = seed ^ 1
	return buf[0] ^ buf[len(buf)-1]
}

//line kernels.hy:70
func K4OpenCodedDefer(x int) (out int) {
	defer func() { out += 1 }()
	return x * 2
}

//line kernels.hy:77
func K5NonLeakingAny(x int) int {
	return sinkNonLeakingAny(x)
}

//line kernels.hy:83
func K6WindowChecksum(buf []byte) uint32 {
	var acc uint32
	for i := 0; i+4 <= len(buf); i += 4 {
		win := buf[i : i+4 : i+4]
		acc ^= uint32(win[0]) | uint32(win[3])<<8
	}
	return acc
}

//line kernels.hy:94
func K7InlinedCleanHelper(buf []byte) byte {
//line kernels.hy:95
	return inlinedXorStep(buf, 0)
}

//line kernels.hy:100
func F1InterfaceBox(x int) int { return sinkLeakingAny(x) }

//line kernels.hy:105
func F2DynamicMake(src []byte, n int) int {
	buf := make([]byte, n)
	copy(buf, src)
	return int(buf[0])
}

//line kernels.hy:113
func F3Const128KiB(seed byte) byte {
	buf := make([]byte, 128*1024)
	buf[0] = seed
	return buf[0]
}

//line kernels.hy:121
func F4LoopDefer(n int) (out int) {
	for i := 0; i < n; i++ {
		defer func() { out++ }()
	}
	return out
}

//go:noinline
//line kernels.hy:131
func F5ReturnedClosure(x int) func() int {
	sum := x
	return func() int { sum++; return sum }
}

//line kernels.hy:139
func F6SliceAppend(n int) int {
	var xs []int
	for i := 0; i < n; i++ {
		xs = append(xs, i)
	}
	return len(xs)
}

//line kernels.hy:149
func F7TransitiveCallee(n int) int { return hiddenHeapCallee(n) }

type byteSource interface{ Next() byte }
type allocReader struct{}

//go:noinline
func (allocReader) Next() byte {
	b := make([]byte, 128*1024)
	return b[0]
}

//line kernels.hy:153
func F8IndirectInterface(s byteSource) byte { return s.Next() }

// Counterexample for Probe #2 (R_CALLIND resolution):
// This frame carries FUNCDATA $4, *.opendefer(SB) from the benign open-coded
// defer, AND also invokes an unaudited first-class funcval `cb(x)` (R_CALLIND).
// Per-callsite R_CALLIND resolution must certify the open-defer epilogue slot
// while still rejecting `cb(x)` at kernels.hy:161.
//line kernels.hy:158
func F9OpenDeferPlusFuncVal(x int, cb func(int) int) (out int) {
//line kernels.hy:159
	defer func() { out += 1 }()
//line kernels.hy:161
	return cb(x)
}

// Second half of Probe #2: open-coded defer whose synthesized `.func1` body
// allocates on the heap (`make([]byte, n)`). Resolving the `.opendefer` slot
// to `F10OpenDeferAllocFuncN.func1` must follow and reject `.func1`.
//line kernels.hy:166
func F10OpenDeferAllocFuncN(n int) (out int) {
//line kernels.hy:167
	defer func() {
//line kernels.hy:168
		b := make([]byte, n)
//line kernels.hy:169
		out += len(b)
	}()
//line kernels.hy:171
	return 1
}

// Probe #1 corner case (`panic*` + `recover()`):
// `runtime.panicIndex` boxes `runtime.boundsError` on the Go heap when caught
// by `recover()` (`1 alloc/op`). Because `runtime.gorecover` / `deferprocStack`
// are excluded from the allowlist, trap-recovery heap allocations fail closed.
//line kernels.hy:176
func F11RecoverBoundsTrap(xs []int, idx int) (out int) {
//line kernels.hy:177
	defer func() {
		if recover() != nil {
			out = -1
		}
	}()
//line kernels.hy:182
	return xs[idx]
}

// Probe #3 worked example (`//line` across inlined bodies):
// `inlinedAppendTag` (kernels.hy:26..28) is inlined into `F12InlinedHelperHyLine`
// (kernels.hy:186..187); the certificate reports both the caller site
// (`kernels.hy:187`) and the exact allocating `.hy` line inside the inlined
// body (`kernels.hy:28: CALL runtime.growslice`).
//line kernels.hy:186
func F12InlinedHelperHyLine(xs []int) int {
//line kernels.hy:187
	ys := inlinedAppendTag(xs, 99)
//line kernels.hy:188
	return len(ys)
}
