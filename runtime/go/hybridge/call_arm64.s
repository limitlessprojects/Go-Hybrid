// Go ABI0 -> AAPCS64 (aarch64) trampolines — the arm64 port of call_amd64.s
// (PLAN row 16, found by the release-matrix fleet: any `rust/`-importing
// program could not link on linux/arm64 until this file existed).
//
// The discipline is the amd64 one (see call_amd64.s for the full design:
// FAST = callee runs on the caller's goroutine stack after an in-trampoline
// stackguard check; DEEP = pooled guarded mmap stack), with three
// arch-specific differences that make the arm64 port STRICTLY SIMPLER:
//
//   1. g lives in R28 on arm64 — the guard check reads g.stackguard0
//      directly (offset pinned by tools/extract_offsets.py against an
//      arm64-built binary; the amd64 file needs a TLS dance, we do not).
//      Toolchain cross-check: cmd/internal/obj/arm64/obj7.go emits
//      `2 * PtrSize` (16) for the same field in every arm64 prologue.
//   2. BL does not push a return address (LR is a register) and Go keeps
//      SP 16-aligned on arm64 — the FAST path needs NO SP adjustment at
//      all (amd64 needs SUB+AND for the SysV alignment gap).
//   3. The DEEP path saves the old Go SP in a 16-byte scratch slot BELOW
//      the callee's entry SP on the pooled stack (a callee never writes at
//      or above its entry SP per AAPCS64) — so, unlike the amd64 port,
//      NO Go callee-saved register is borrowed. LR (R30) is saved/restored
//      by the assembler's own prologue/epilogue (declared frame > 0).
//
// Trampoline discipline (mirrors call_amd64.s rules 1-5):
//   1. load all Go-frame inputs BEFORE any SP adjustment;
//   2. SP restored BEFORE writing results to the Go frame;
//   3. 16-byte AAPCS64 alignment at callee entry (SP%16==0 — arm64 has no
//      ret-addr push, so the callee entry SP is the caller's SP as-is);
//   4. NOSPLIT: no Go stack growth inside;
//   5. no callbacks into Go while on a deep stack; panics abort at the
//      Rust edge (E9) — they never unwind across.
//
// SysV arg/return mapping: X0-X3 args, X0 return (Go ABI0 frame args are
// read via the FP pseudo-registers; the assembler pins their real offsets).

#include "textflag.h"

// go1.26.9 linux/arm64 runtime offsets (extracted by tools/extract_offsets.py
// run against an arm64-built binary — see the offsets note in
// interfaces/abi-types.md; a Go version bump regenerates them; the canary is
// the SPHeadroomForTest offsets test, which exercises this constant on both
// GOARCHes).
#define OFF_g_stackguard0 16

// func spHeadroom() uintptr  — SP - g.stackguard0 (bytes left above guard)
TEXT ·spHeadroom(SB), NOSPLIT, $0-8
	MOVD	OFF_g_stackguard0(g), R10
	MOVD	RSP, R11
	CMP	R10, R11
	BLO	headroomzero
	SUB	R10, R11
	MOVD	R11, ret+0(FP)
	RET
headroomzero:
	MOVD	ZR, ret+0(FP)
	RET

// ------------------------------- fast path (caller's goroutine stack)
// The stackguard check is INLINED here (what the bridge generator emits per
// call site). If headroom < guard, the callee is NOT invoked: the sentinel
// NEEDS_DEEP (-2 = ^uint64(1)) is returned and the Go wrapper retries on a
// pooled deep stack. The headroom is measured AFTER the assembler's own
// LR/FP prologue, i.e. 32 bytes conservative — same ordering as amd64.

// func callFast1(fn unsafe.Pointer, a1 uintptr) uint64
TEXT ·callFast1(SB), NOSPLIT, $16-24
	MOVD	fn+0(FP), R9
	MOVD	a1+8(FP), R0
	MOVD	OFF_g_stackguard0(g), R10
	MOVD	RSP, R11
	CMP	R10, R11
	BLO	needsdeep1
	SUB	R10, R11
	CMP	$4096, R11
	BLO	needsdeep1
	BL	(R9)
	MOVD	R0, ret+16(FP)
	RET
needsdeep1:
	MOVD	$-2, R0
	MOVD	R0, ret+16(FP)
	RET

// func callFast2(fn unsafe.Pointer, a1, a2 uintptr) uint64
TEXT ·callFast2(SB), NOSPLIT, $16-32
	MOVD	fn+0(FP), R9
	MOVD	a1+8(FP), R0
	MOVD	a2+16(FP), R1
	MOVD	OFF_g_stackguard0(g), R10
	MOVD	RSP, R11
	CMP	R10, R11
	BLO	needsdeep2
	SUB	R10, R11
	CMP	$4096, R11
	BLO	needsdeep2
	BL	(R9)
	MOVD	R0, ret+24(FP)
	RET
needsdeep2:
	MOVD	$-2, R0
	MOVD	R0, ret+24(FP)
	RET

// func callFast3(fn unsafe.Pointer, a1, a2, a3 uintptr) uint64
TEXT ·callFast3(SB), NOSPLIT, $16-40
	MOVD	fn+0(FP), R9
	MOVD	a1+8(FP), R0
	MOVD	a2+16(FP), R1
	MOVD	a3+24(FP), R2
	MOVD	OFF_g_stackguard0(g), R10
	MOVD	RSP, R11
	CMP	R10, R11
	BLO	needsdeep3
	SUB	R10, R11
	CMP	$4096, R11
	BLO	needsdeep3
	BL	(R9)
	MOVD	R0, ret+32(FP)
	RET
needsdeep3:
	MOVD	$-2, R0
	MOVD	R0, ret+32(FP)
	RET

// func callFast4(fn unsafe.Pointer, a1, a2, a3, a4 uintptr) uint64
TEXT ·callFast4(SB), NOSPLIT, $16-48
	MOVD	fn+0(FP), R9
	MOVD	a1+8(FP), R0
	MOVD	a2+16(FP), R1
	MOVD	a3+24(FP), R2
	MOVD	a4+32(FP), R3
	MOVD	OFF_g_stackguard0(g), R10
	MOVD	RSP, R11
	CMP	R10, R11
	BLO	needsdeep4
	SUB	R10, R11
	CMP	$4096, R11
	BLO	needsdeep4
	BL	(R9)
	MOVD	R0, ret+40(FP)
	RET
needsdeep4:
	MOVD	$-2, R0
	MOVD	R0, ret+40(FP)
	RET

// ------------------------------- small-guard fast path (row 4: per-callee budgets)
// Identical discipline to callFastN above, but the guard is $1024 instead of
// $4096: callees whose descriptor budget is <= 1 KiB (bounded, non-recursive,
// measured/argued in interfaces/descriptors.md) take the fast path on
// SHALLOWER stacks. Everything else is byte-for-byte the same rules.

// func callFast1S(fn unsafe.Pointer, a1 uintptr) uint64
TEXT ·callFast1S(SB), NOSPLIT, $16-24
	MOVD	fn+0(FP), R9
	MOVD	a1+8(FP), R0
	MOVD	OFF_g_stackguard0(g), R10
	MOVD	RSP, R11
	CMP	R10, R11
	BLO	needsdeep1s
	SUB	R10, R11
	CMP	$1024, R11
	BLO	needsdeep1s
	BL	(R9)
	MOVD	R0, ret+16(FP)
	RET
needsdeep1s:
	MOVD	$-2, R0
	MOVD	R0, ret+16(FP)
	RET

// func callFast2S(fn unsafe.Pointer, a1, a2 uintptr) uint64
TEXT ·callFast2S(SB), NOSPLIT, $16-32
	MOVD	fn+0(FP), R9
	MOVD	a1+8(FP), R0
	MOVD	a2+16(FP), R1
	MOVD	OFF_g_stackguard0(g), R10
	MOVD	RSP, R11
	CMP	R10, R11
	BLO	needsdeep2s
	SUB	R10, R11
	CMP	$1024, R11
	BLO	needsdeep2s
	BL	(R9)
	MOVD	R0, ret+24(FP)
	RET
needsdeep2s:
	MOVD	$-2, R0
	MOVD	R0, ret+24(FP)
	RET

// func callFast3S(fn unsafe.Pointer, a1, a2, a3 uintptr) uint64
TEXT ·callFast3S(SB), NOSPLIT, $16-40
	MOVD	fn+0(FP), R9
	MOVD	a1+8(FP), R0
	MOVD	a2+16(FP), R1
	MOVD	a3+24(FP), R2
	MOVD	OFF_g_stackguard0(g), R10
	MOVD	RSP, R11
	CMP	R10, R11
	BLO	needsdeep3s
	SUB	R10, R11
	CMP	$1024, R11
	BLO	needsdeep3s
	BL	(R9)
	MOVD	R0, ret+32(FP)
	RET
needsdeep3s:
	MOVD	$-2, R0
	MOVD	R0, ret+32(FP)
	RET

// func callFast4S(fn unsafe.Pointer, a1, a2, a3, a4 uintptr) uint64
TEXT ·callFast4S(SB), NOSPLIT, $16-48
	MOVD	fn+0(FP), R9
	MOVD	a1+8(FP), R0
	MOVD	a2+16(FP), R1
	MOVD	a3+24(FP), R2
	MOVD	a4+32(FP), R3
	MOVD	OFF_g_stackguard0(g), R10
	MOVD	RSP, R11
	CMP	R10, R11
	BLO	needsdeep4s
	SUB	R10, R11
	CMP	$1024, R11
	BLO	needsdeep4s
	BL	(R9)
	MOVD	R0, ret+40(FP)
	RET
needsdeep4s:
	MOVD	$-2, R0
	MOVD	R0, ret+40(FP)
	RET

// ------------------------------- deep path (pooled guarded mmap stack)
// stack = pooled stack TOP (page-multiple, hence 16-aligned). The old Go SP
// is stashed in a 16-byte scratch slot at TOP-16 (below the callee's entry
// SP — an AAPCS64 callee never writes at or above its entry SP, and its
// frame grows DOWN from there), so no callee-saved register is borrowed.
// After the callee returns, its SP is back at its entry (the scratch base);
// we reload the Go SP from there, then write results.

// func callDeep1(fn unsafe.Pointer, a1 uintptr, stack uintptr) uint64
TEXT ·callDeep1(SB), NOSPLIT, $16-32
	MOVD	fn+0(FP), R9
	MOVD	a1+8(FP), R0
	MOVD	stack+16(FP), R10
	SUB	$16, R10
	MOVD	RSP, R12
	MOVD	R12, 0(R10)
	MOVD	R10, RSP
	BL	(R9)
	MOVD	0(RSP), R12
	MOVD	R12, RSP
	MOVD	R0, ret+24(FP)
	RET

// func callDeep2(fn unsafe.Pointer, a1, a2 uintptr, stack uintptr) uint64
TEXT ·callDeep2(SB), NOSPLIT, $16-40
	MOVD	fn+0(FP), R9
	MOVD	a1+8(FP), R0
	MOVD	a2+16(FP), R1
	MOVD	stack+24(FP), R10
	SUB	$16, R10
	MOVD	RSP, R12
	MOVD	R12, 0(R10)
	MOVD	R10, RSP
	BL	(R9)
	MOVD	0(RSP), R12
	MOVD	R12, RSP
	MOVD	R0, ret+32(FP)
	RET

// func callDeep3(fn unsafe.Pointer, a1, a2, a3 uintptr, stack uintptr) uint64
TEXT ·callDeep3(SB), NOSPLIT, $16-48
	MOVD	fn+0(FP), R9
	MOVD	a1+8(FP), R0
	MOVD	a2+16(FP), R1
	MOVD	a3+24(FP), R2
	MOVD	stack+32(FP), R10
	SUB	$16, R10
	MOVD	RSP, R12
	MOVD	R12, 0(R10)
	MOVD	R10, RSP
	BL	(R9)
	MOVD	0(RSP), R12
	MOVD	R12, RSP
	MOVD	R0, ret+40(FP)
	RET

// func callDeep4(fn unsafe.Pointer, a1, a2, a3, a4 uintptr, stack uintptr) uint64
TEXT ·callDeep4(SB), NOSPLIT, $16-56
	MOVD	fn+0(FP), R9
	MOVD	a1+8(FP), R0
	MOVD	a2+16(FP), R1
	MOVD	a3+24(FP), R2
	MOVD	a4+32(FP), R3
	MOVD	stack+40(FP), R10
	SUB	$16, R10
	MOVD	RSP, R12
	MOVD	R12, 0(R10)
	MOVD	R10, RSP
	BL	(R9)
	MOVD	0(RSP), R12
	MOVD	R12, RSP
	MOVD	R0, ret+48(FP)
	RET
