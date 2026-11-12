package sha3binding

import (
	"errors"
	"runtime"
	"unsafe"

	"hybrid.program/hybridge"
)

// The binding's error surface (Spike 5's sentinel shape): Go-side constants,
// mapped from the C-ABI status word — wrap-on-demand, ≤1 allocation (E5).
var (
	ErrDigest = errors.New("sha3: digest256 failed")
	ErrABI    = errors.New("sha3: hyrt ABI handshake mismatch (rebuild the runtime)")
)

func init() {
	// SMALL class (guard 1024, row 4): the abi-word probe is a constant
	// return — zero recursion, zero heap, a handful of bytes of frame.
	if w := hybridge.Cross1S(FnAbiWord, 0); w != abiWordExpected {
		panic(ErrABI.Error())
	}
}

// Digest256 computes SHA3-256(data) — the pinned-borrow protocol in full:
//
//	pin data's backing array → cross (ptr, len, out) → unpin + KeepAlive.
//
// The 32-byte digest buffer is GO-owned (allocated here, written by Rust
// through the pinned pointer during the call; the pointer is never
// retained — abi-types.md §2). Empty input is legal (SHA3 of the empty
// string); the nil-data leg passes 0/0.
func Digest256(data []byte) ([]byte, error) {
	out := make([]byte, 32)
	var p runtime.Pinner
	var dataPtr uintptr
	if len(data) > 0 {
		dataPtr = uintptr(unsafe.Pointer(&data[0]))
		p.Pin(&data[0])
	}
	p.Pin(&out[0])
	// SMALL class (guard 1024, row 4): Keccak-f[1600] absorb/squeeze is a
	// fixed 200-byte state permutation in a loop — no recursion, no
	// input-proportional stack (interfaces/descriptors.md sha3 rows).
	st := hybridge.Cross3S(FnDigest256, dataPtr, uintptr(len(data)),
		uintptr(unsafe.Pointer(&out[0])))
	p.Unpin()
	runtime.KeepAlive(&p)
	runtime.KeepAlive(data)
	runtime.KeepAlive(out)
	if st != 0 {
		return nil, ErrDigest
	}
	return out, nil
}

// Digest256Borrowed is the zero-copy, pin-free variant of Digest256 for a
// borrowed (non-escaping, caller-exclusive) `data` view — the shape a Tier-0
// loan certificate would license (issue #3429).
//
// Why no runtime.Pinner for data: the crossing is a synchronous NOSPLIT
// trampoline on the calling goroutine, so that goroutine cannot reach a Go
// safepoint (and so cannot have its stack grown, shrunk or moved) until Rust
// returns; a heap backing array never moves under Go's non-moving GC, and
// KeepAlive holds it live across the call. Rust must not retain the pointer
// (abi-types.md §2). The 32-byte output is still Go-owned and written by
// Rust; it stays pinned-free for the same reasons. No payload byte is
// copied; the Spike 3 P3 pin cost (~55 ns/crossing) is not paid.
//
// NOT YET MEASURED: the issue's ≥3x/0-alloc benchmark and the GC / stack-move
// stress runs are pending (no Go/Rust toolchain was available when this was
// written) — do not treat the throughput claim as verified.
func Digest256Borrowed(data []byte) ([]byte, error) {
	out := make([]byte, 32)
	var dataPtr uintptr
	if len(data) > 0 {
		dataPtr = uintptr(unsafe.Pointer(&data[0]))
	}
	st := hybridge.Cross3S(FnDigest256, dataPtr, uintptr(len(data)),
		uintptr(unsafe.Pointer(&out[0])))
	runtime.KeepAlive(data)
	runtime.KeepAlive(out)
	if st != 0 {
		return nil, ErrDigest
	}
	return out, nil
}

// StreamChecksum runs `iters` SHA3-256 permutations INSIDE Rust (one
// boundary crossing — the hot-loop shape example 10 demonstrates) and
// returns the deterministic fold. No pointers cross. SMALL class (guard
// 1024, row 4): the callee's stack use is a CONSTANT (one permutation's
// frame; the loop carries no growth) — bounded-stack callee, and now
// eligible on SHALLOW stacks too.
func StreamChecksum(iters, seed uint64) (uint64, error) {
	v := hybridge.Cross2S(FnStreamChecksum, uintptr(iters), uintptr(seed))
	return v, nil
}
