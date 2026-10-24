//go:build windows && amd64

// Go ABI0 -> Win64 amd64 trampolines (PLAN row 17): the Windows twin of
// call_amd64.s (SysV). Same crossing design — FAST path on the caller's own
// goroutine stack after an in-trampoline stackguard check; DEEP path on the
// pooled guarded stacks from stack_windows.go (kernel32 VirtualAlloc pool).
//
// The Win64 differences (Microsoft x64 calling convention — what Rust's
// `extern "C"` uses on windows-gnu):
//   * integer args go in RCX, RDX, R8, R9 (SysV: RDI, RSI, RDX, RCX);
//   * the caller reserves a 32-byte SHADOW SPACE above the call (home area
//     for the callee's first four args) — 32 bytes even when fewer args;
//   * RSP must be 16-byte aligned at the CALL instruction;
//   * RBX, RDI, RSI, RBP, R12-R15, XMM6-15 are NON-VOLATILE (callee-saved).
//     Saving the caller's SP in RBX across the CALL is exactly as safe as
//     under SysV: the Rust callee preserves RBX by ABI.
//
// Trampoline discipline (identical rules, both files):
//   1. load all Go-frame inputs BEFORE any SP adjustment;
//   2. save old SP in RBX; restore BEFORE writing results to the Go frame;
//   3. 16-byte alignment + 32-byte shadow space at every CALL;
//   4. NOSPLIT: no Go stack growth inside;
//   5. the fast path returns the NEEDS_DEEP sentinel ($-2) when headroom is
//      short — the Go wrapper retries on a pooled deep stack.
//
// The g/TLS read is the runtime's own get_tls pattern — the same code the
// runtime's asm_amd64.s executes on windows/amd64 in both linking modes
// (internal and cgo-external), so no load_g/save_g dance is needed here.


#include "textflag.h"

// get_tls/g macros inlined from $GOROOT/src/runtime/go_tls.h (GOARCH_amd64):
// the assembler's include path does not carry go_tls.h for user packages.
#define get_tls(r)      MOVQ TLS, r
#define g(r)            0(r)(TLS*1)

// windows/amd64 runtime offsets (same extraction discipline as the SysV
// file: ../extract_offsets.py against a windows/amd64 binary; CI canary is
// the offsets test).
#define OFF_g_stackguard0 16

// func spHeadroom() uintptr  — SP - g.stackguard0 (bytes left above guard)
TEXT ·spHeadroom(SB), NOSPLIT, $0-8
        get_tls(BX)
        MOVQ    g(BX), BX
        MOVQ    OFF_g_stackguard0(BX), AX
        MOVQ    SP, DX
        SUBQ    AX, DX
        JCC     headroomok
        XORQ    DX, DX
headroomok:
        MOVQ    DX, ret+0(FP)
        RET

// ------------------------------- fast path (caller's goroutine stack)
// ALIGNMENT CONTRACT (fleet-proven twice, sha3 crashes 2026-10-14): Go's
// ABI0 does NOT guarantee 16-byte alignment at a call site — the entry
// residue varies with the caller's frame layout, and gcc-built callees
// zero their frames with 16-byte-aligned SSE (movaps), which faults on a
// misaligned RSP. The fast path therefore FORCES alignment exactly like
// the SysV file does: ANDQ $-16 (round down), then 32 bytes of shadow
// space (≡0 mod 16 at the CALL, the Win64 requirement). Two fixed-sub
// attempts ($40, then $48) each crashed on a different callee because
// each silently assumed a constant entry residue. The deep path was
// always correct (page-aligned top + ANDQ $-16).

// func callFast1(fn unsafe.Pointer, a1 uintptr) uint64
TEXT ·callFast1(SB), NOSPLIT, $0-24
        MOVQ    fn+0(FP), AX
        MOVQ    a1+8(FP), CX
        get_tls(BX)
        MOVQ    g(BX), BX
        MOVQ    OFF_g_stackguard0(BX), DX
        MOVQ    SP, R11
        SUBQ    DX, R11
        JCS     needsdeep1
        CMPQ    R11, $4096
        JCS     needsdeep1
        MOVQ    SP, BX
        ANDQ    $-16, SP
        SUBQ    $32, SP
        CALL    AX
        MOVQ    BX, SP
        MOVQ    AX, ret+16(FP)
        RET
needsdeep1:
        MOVQ    $-2, AX
        MOVQ    AX, ret+16(FP)
        RET

// func callFast2(fn unsafe.Pointer, a1, a2 uintptr) uint64
TEXT ·callFast2(SB), NOSPLIT, $0-32
        MOVQ    fn+0(FP), AX
        MOVQ    a1+8(FP), CX
        MOVQ    a2+16(FP), DX
        get_tls(BX)
        MOVQ    g(BX), BX
        MOVQ    OFF_g_stackguard0(BX), R11
        MOVQ    SP, R12
        SUBQ    R11, R12
        JCS     needsdeep2
        CMPQ    R12, $4096
        JCS     needsdeep2
        MOVQ    SP, BX
        ANDQ    $-16, SP
        SUBQ    $32, SP
        CALL    AX
        MOVQ    BX, SP
        MOVQ    AX, ret+24(FP)
        RET
needsdeep2:
        MOVQ    $-2, AX
        MOVQ    AX, ret+24(FP)
        RET

// func callFast3(fn unsafe.Pointer, a1, a2, a3 uintptr) uint64
TEXT ·callFast3(SB), NOSPLIT, $0-40
        MOVQ    fn+0(FP), AX
        MOVQ    a1+8(FP), CX
        MOVQ    a2+16(FP), DX
        MOVQ    a3+24(FP), R8
        get_tls(BX)
        MOVQ    g(BX), BX
        MOVQ    OFF_g_stackguard0(BX), R11
        MOVQ    SP, R12
        SUBQ    R11, R12
        JCS     needsdeep3
        CMPQ    R12, $4096
        JCS     needsdeep3
        MOVQ    SP, BX
        ANDQ    $-16, SP
        SUBQ    $32, SP
        CALL    AX
        MOVQ    BX, SP
        MOVQ    AX, ret+32(FP)
        RET
needsdeep3:
        MOVQ    $-2, AX
        MOVQ    AX, ret+32(FP)
        RET

// func callFast4(fn unsafe.Pointer, a1, a2, a3, a4 uintptr) uint64
TEXT ·callFast4(SB), NOSPLIT, $0-48
        MOVQ    fn+0(FP), AX
        MOVQ    a1+8(FP), CX
        MOVQ    a2+16(FP), DX
        MOVQ    a3+24(FP), R8
        MOVQ    a4+32(FP), R9
        get_tls(BX)
        MOVQ    g(BX), BX
        MOVQ    OFF_g_stackguard0(BX), R11
        MOVQ    SP, R12
        SUBQ    R11, R12
        JCS     needsdeep4
        CMPQ    R12, $4096
        JCS     needsdeep4
        MOVQ    SP, BX
        ANDQ    $-16, SP
        SUBQ    $32, SP
        CALL    AX
        MOVQ    BX, SP
        MOVQ    AX, ret+40(FP)
        RET
needsdeep4:
        MOVQ    $-2, AX
        MOVQ    AX, ret+40(FP)
        RET

// ------------------------------- small-guard fast path (row 4)
// Same discipline, guard $1024: callees whose descriptor budget is <= 1 KiB
// take the fast path on shallower stacks. Byte-for-byte the Win64 rules.

// func callFast1S(fn unsafe.Pointer, a1 uintptr) uint64
TEXT ·callFast1S(SB), NOSPLIT, $0-24
        MOVQ    fn+0(FP), AX
        MOVQ    a1+8(FP), CX
        get_tls(BX)
        MOVQ    g(BX), BX
        MOVQ    OFF_g_stackguard0(BX), DX
        MOVQ    SP, R11
        SUBQ    DX, R11
        JCS     needsdeep1s
        CMPQ    R11, $1024
        JCS     needsdeep1s
        MOVQ    SP, BX
        ANDQ    $-16, SP
        SUBQ    $32, SP
        CALL    AX
        MOVQ    BX, SP
        MOVQ    AX, ret+16(FP)
        RET
needsdeep1s:
        MOVQ    $-2, AX
        MOVQ    AX, ret+16(FP)
        RET

// func callFast2S(fn unsafe.Pointer, a1, a2 uintptr) uint64
TEXT ·callFast2S(SB), NOSPLIT, $0-32
        MOVQ    fn+0(FP), AX
        MOVQ    a1+8(FP), CX
        MOVQ    a2+16(FP), DX
        get_tls(BX)
        MOVQ    g(BX), BX
        MOVQ    OFF_g_stackguard0(BX), R11
        MOVQ    SP, R12
        SUBQ    R11, R12
        JCS     needsdeep2s
        CMPQ    R12, $1024
        JCS     needsdeep2s
        MOVQ    SP, BX
        ANDQ    $-16, SP
        SUBQ    $32, SP
        CALL    AX
        MOVQ    BX, SP
        MOVQ    AX, ret+24(FP)
        RET
needsdeep2s:
        MOVQ    $-2, AX
        MOVQ    AX, ret+24(FP)
        RET

// func callFast3S(fn unsafe.Pointer, a1, a2, a3 uintptr) uint64
TEXT ·callFast3S(SB), NOSPLIT, $0-40
        MOVQ    fn+0(FP), AX
        MOVQ    a1+8(FP), CX
        MOVQ    a2+16(FP), DX
        MOVQ    a3+24(FP), R8
        get_tls(BX)
        MOVQ    g(BX), BX
        MOVQ    OFF_g_stackguard0(BX), R11
        MOVQ    SP, R12
        SUBQ    R11, R12
        JCS     needsdeep3s
        CMPQ    R12, $1024
        JCS     needsdeep3s
        MOVQ    SP, BX
        ANDQ    $-16, SP
        SUBQ    $32, SP
        CALL    AX
        MOVQ    BX, SP
        MOVQ    AX, ret+32(FP)
        RET
needsdeep3s:
        MOVQ    $-2, AX
        MOVQ    AX, ret+32(FP)
        RET

// func callFast4S(fn unsafe.Pointer, a1, a2, a3, a4 uintptr) uint64
TEXT ·callFast4S(SB), NOSPLIT, $0-48
        MOVQ    fn+0(FP), AX
        MOVQ    a1+8(FP), CX
        MOVQ    a2+16(FP), DX
        MOVQ    a3+24(FP), R8
        MOVQ    a4+32(FP), R9
        get_tls(BX)
        MOVQ    g(BX), BX
        MOVQ    OFF_g_stackguard0(BX), R11
        MOVQ    SP, R12
        SUBQ    R11, R12
        JCS     needsdeep4s
        CMPQ    R12, $1024
        JCS     needsdeep4s
        MOVQ    SP, BX
        ANDQ    $-16, SP
        SUBQ    $32, SP
        CALL    AX
        MOVQ    BX, SP
        MOVQ    AX, ret+40(FP)
        RET
needsdeep4s:
        MOVQ    $-2, AX
        MOVQ    AX, ret+40(FP)
        RET

// ------------------------------- deep path (pooled guarded stack)
// The pooled stack top is page-aligned (16-aligned): ANDQ kept for
// defensiveness, then 32 bytes of shadow space. SP%16 == 0 at CALL.

// func callDeep1(fn unsafe.Pointer, a1 uintptr, stack uintptr) uint64
TEXT ·callDeep1(SB), NOSPLIT, $0-32
        MOVQ    fn+0(FP), AX
        MOVQ    a1+8(FP), CX
        MOVQ    stack+16(FP), R11
        MOVQ    SP, BX
        MOVQ    R11, SP
        ANDQ    $-16, SP
        SUBQ    $32, SP
        CALL    AX
        MOVQ    BX, SP
        MOVQ    AX, ret+24(FP)
        RET

// func callDeep2(fn unsafe.Pointer, a1, a2 uintptr, stack uintptr) uint64
TEXT ·callDeep2(SB), NOSPLIT, $0-40
        MOVQ    fn+0(FP), AX
        MOVQ    a1+8(FP), CX
        MOVQ    a2+16(FP), DX
        MOVQ    stack+24(FP), R11
        MOVQ    SP, BX
        MOVQ    R11, SP
        ANDQ    $-16, SP
        SUBQ    $32, SP
        CALL    AX
        MOVQ    BX, SP
        MOVQ    AX, ret+32(FP)
        RET

// func callDeep3(fn unsafe.Pointer, a1, a2, a3 uintptr, stack uintptr) uint64
TEXT ·callDeep3(SB), NOSPLIT, $0-48
        MOVQ    fn+0(FP), AX
        MOVQ    a1+8(FP), CX
        MOVQ    a2+16(FP), DX
        MOVQ    a3+24(FP), R8
        MOVQ    stack+32(FP), R11
        MOVQ    SP, BX
        MOVQ    R11, SP
        ANDQ    $-16, SP
        SUBQ    $32, SP
        CALL    AX
        MOVQ    BX, SP
        MOVQ    AX, ret+40(FP)
        RET

// func callDeep4(fn unsafe.Pointer, a1, a2, a3, a4 uintptr, stack uintptr) uint64
TEXT ·callDeep4(SB), NOSPLIT, $0-56
        MOVQ    fn+0(FP), AX
        MOVQ    a1+8(FP), CX
        MOVQ    a2+16(FP), DX
        MOVQ    a3+24(FP), R8
        MOVQ    a4+32(FP), R9
        MOVQ    stack+40(FP), R11
        MOVQ    SP, BX
        MOVQ    R11, SP
        ANDQ    $-16, SP
        SUBQ    $32, SP
        CALL    AX
        MOVQ    BX, SP
        MOVQ    AX, ret+48(FP)
        RET
