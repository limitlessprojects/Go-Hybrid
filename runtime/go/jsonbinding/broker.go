// Package jsonbinding — the Go side of the `rust/serde_json` bridge
// (example 05). Generated-shape file: what the bridge generator emits for a
// pinned crate whose values are RUST-OWNED HANDLES (abi-types.md §2).
//
// A jsonbinding.Value is a generation-tagged handle (u64 = (gen<<32)|(idx+1))
// into hyrt's owning-side table. Go never touches JSON memory; every method
// crosses through the trampolines. Release runs via runtime.AddCleanup when
// the wrapper becomes unreachable; a MOVED value (Set's argument) has its id
// zeroed first, so the late cleanup releases nothing (idempotent anyway).
//
// Key strings cross as the pinned-borrow protocol: []byte(key) copy →
// pin → cross → unpin + KeepAlive (copy-small; keys are small).
package jsonbinding

/*
#cgo LDFLAGS: -L${SRCDIR}/../lib -lhyrt
#cgo !windows LDFLAGS: -lpthread -ldl -lm
#cgo windows LDFLAGS: -lwinpthread -lntdll -lws2_32 -lbcrypt -luserenv -lole32 -loleaut32 -lsynchronization -ladvapi32
#include <stdint.h>

extern uint64_t hy_json_parse(uint64_t ptr, uint64_t len, uint64_t out_handle);
extern uint64_t hy_json_new_object(uint64_t out_handle);
extern uint64_t hy_json_new_array(uint64_t out_handle);
extern uint64_t hy_json_get_array(uint64_t v, uint64_t kptr, uint64_t klen, uint64_t out_handle);
extern uint64_t hy_json_get_int(uint64_t v, uint64_t kptr, uint64_t klen, uint64_t out);
extern uint64_t hy_json_get_string(uint64_t v, uint64_t kptr, uint64_t klen, uint64_t out_handle);
extern uint64_t hy_json_array_len(uint64_t arr, uint64_t out);
extern uint64_t hy_json_array_get(uint64_t arr, uint64_t i, uint64_t out_handle);
extern uint64_t hy_json_array_push_string(uint64_t arr, uint64_t sptr, uint64_t slen);
extern uint64_t hy_json_set(uint64_t v, uint64_t kptr, uint64_t klen, uint64_t value);
extern uint64_t hy_json_to_string_pretty(uint64_t v, uint64_t out, uint64_t cap, uint64_t out_len);
extern uint64_t hy_json_release(uint64_t h);
extern uint64_t hyrt_abi_word(void);

static void* a_parse(void)        { return (void*)&hy_json_parse; }
static void* a_new_object(void)   { return (void*)&hy_json_new_object; }
static void* a_new_array(void)    { return (void*)&hy_json_new_array; }
static void* a_get_array(void)    { return (void*)&hy_json_get_array; }
static void* a_get_int(void)      { return (void*)&hy_json_get_int; }
static void* a_get_string(void)   { return (void*)&hy_json_get_string; }
static void* a_array_len(void)    { return (void*)&hy_json_array_len; }
static void* a_array_get(void)    { return (void*)&hy_json_array_get; }
static void* a_push_string(void)  { return (void*)&hy_json_array_push_string; }
static void* a_set(void)          { return (void*)&hy_json_set; }
static void* a_to_pretty(void)    { return (void*)&hy_json_to_string_pretty; }
static void* a_release(void)      { return (void*)&hy_json_release; }
static void* a_abi_word(void)     { return (void*)&hyrt_abi_word; }
*/
import "C"

import "unsafe"

import "hybrid.program/hybridge"

var (
	FnParse          = unsafe.Pointer(C.a_parse())
	FnNewObject      = unsafe.Pointer(C.a_new_object())
	FnNewArray       = unsafe.Pointer(C.a_new_array())
	FnGetArray       = unsafe.Pointer(C.a_get_array())
	FnGetInt         = unsafe.Pointer(C.a_get_int())
	FnGetString      = unsafe.Pointer(C.a_get_string())
	FnArrayLen       = unsafe.Pointer(C.a_array_len())
	FnArrayGet       = unsafe.Pointer(C.a_array_get())
	FnPushString     = unsafe.Pointer(C.a_push_string())
	FnSet            = unsafe.Pointer(C.a_set())
	FnToStringPretty = unsafe.Pointer(C.a_to_pretty())
	FnRelease        = unsafe.Pointer(C.a_release())
	FnAbiWord        = unsafe.Pointer(C.a_abi_word())
)

// ABI handshake — same generation word as sha3binding's (one hyrt).
const abiWordExpected = 0x68795f61626931

// Status codes hyrt's json exports return (see runtime/rust/hyrt/src/lib.rs).
const (
	statusOK       = 0
	statusTryAgain = 3
)

func init() {
	// SMALL class (guard 1024, row 4): the abi-word probe is a constant
	// return — zero recursion, zero heap, a handful of bytes of frame.
	if w := hybridge.Cross1S(FnAbiWord, 0); w != abiWordExpected {
		panic("jsonbinding: hyrt ABI handshake mismatch (rebuild the runtime)")
	}
}
