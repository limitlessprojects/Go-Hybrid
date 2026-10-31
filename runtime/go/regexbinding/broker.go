// Package regexbinding — the Go side of the `rust/regex` bridge (example
// 09). Generated-shape file: the pattern and text cross as pinned borrows;
// the result comes back in RAX (status-in-RAX: 1/0/-1), no out-pointers.
package regexbinding

/*
#cgo LDFLAGS: -L${SRCDIR}/../lib -lhyrt
#cgo !windows LDFLAGS: -lpthread -ldl -lm
#cgo windows LDFLAGS: -lwinpthread -lntdll -lws2_32 -lbcrypt -luserenv -lole32 -loleaut32 -lsynchronization -ladvapi32
#include <stdint.h>

extern uint64_t hy_regex_is_match(uint64_t pptr, uint64_t plen, uint64_t tptr, uint64_t tlen);
extern uint64_t hy_regex_is_valid(uint64_t pptr, uint64_t plen);
extern uint64_t hyrt_abi_word(void);

static void* a_is_match(void) { return (void*)&hy_regex_is_match; }
static void* a_is_valid(void) { return (void*)&hy_regex_is_valid; }
static void* a_abi_word(void) { return (void*)&hyrt_abi_word; }
*/
import "C"

import "unsafe"

import "hybrid.program/hybridge"

var (
	FnIsMatch = unsafe.Pointer(C.a_is_match())
	FnIsValid = unsafe.Pointer(C.a_is_valid())
	FnAbiWord = unsafe.Pointer(C.a_abi_word())
)

// ABI handshake — same generation word as the other bindings (one hyrt).
const abiWordExpected = 0x68795f61626931

func init() {
	// SMALL class (guard 1024, row 4): the abi-word probe is a constant
	// return — zero recursion, zero heap, a handful of bytes of frame.
	if w := hybridge.Cross1S(FnAbiWord, 0); w != abiWordExpected {
		panic("regexbinding: hyrt ABI handshake mismatch (rebuild the runtime)")
	}
}
