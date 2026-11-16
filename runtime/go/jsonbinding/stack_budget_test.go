package jsonbinding

// Row 4 stack-budget regression — MEASURED, not argued.
//
// interfaces/descriptors.md lets a fn use the SMALL fast path (guard
// hybridge.SmallGuard = 1024 B) only if its real worst-case stack use stays
// at or below that guard: the guard proves headroom at ENTRY and nothing
// after. These tests paint a pooled stack with a sentinel, run each
// SMALL-routed fn on it, and read the high-water mark back
// (hybridge.StackHighWaterForTest). Allocation growth is what moves the
// number — a Vec<Slot> realloc in the handle table drags libc malloc's frames
// onto the caller's stack — so every fn is driven across table growth
// (hundreds of live handles), not just a cold call.

import (
	"testing"
	"unsafe"

	"hybrid.program/hybridge"
)

// perClassLimit is the guard of the trampoline class the fn is routed
// through in json.go. A fn moves between rows only with descriptors.md.
const (
	smallLimit   = uintptr(hybridge.SmallGuard) // Cross*S
	defaultLimit = uintptr(4096)                // Cross* (hybridge.fastStackNeed)
)

func TestSmallRoutedCalleesStayUnderTheirGuard(t *testing.T) {
	type probe struct {
		limit uintptr
		worst uintptr
	}
	p := map[string]*probe{
		"FnNewObject":  {limit: smallLimit},
		"FnNewArray":   {limit: smallLimit},
		"FnGetInt":     {limit: smallLimit},
		"FnGetString":  {limit: smallLimit},
		"FnArrayLen":   {limit: smallLimit},
		"FnPushString": {limit: smallLimit},
		"FnAbiWord":    {limit: smallLimit},
	}
	measure := func(name string, fn unsafe.Pointer, a1, a2, a3, a4 uintptr) {
		t.Helper()
		_, hw := hybridge.StackHighWaterForTest(fn, a1, a2, a3, a4)
		if hw > p[name].worst {
			p[name].worst = hw
		}
	}

	doc, err := Parse([]byte(`{"k": 42, "name": "hy"}`))
	if err != nil {
		t.Fatalf("Parse: %v", err)
	}
	docID := uintptr(doc.h.id)
	key := []byte("k")
	nameKey := []byte("name")
	text := []byte("ab")
	n := new(uint64)
	i := new(int64)
	var ids []uint64

	arr := new(uint64)
	measure("FnNewArray", FnNewArray, uintptr(unsafe.Pointer(arr)), 0, 0, 0)

	// 600 rounds: past the handle table's 129th and 257th live handle (the
	// Vec growth steps) and the array's growth steps.
	for range 600 {
		measure("FnNewObject", FnNewObject, uintptr(unsafe.Pointer(n)), 0, 0, 0)
		ids = append(ids, *n)
		measure("FnNewArray", FnNewArray, uintptr(unsafe.Pointer(n)), 0, 0, 0)
		ids = append(ids, *n)
		measure("FnGetString", FnGetString, docID, uintptr(unsafe.Pointer(&nameKey[0])), uintptr(len(nameKey)), uintptr(unsafe.Pointer(n)))
		ids = append(ids, *n)
		measure("FnGetInt", FnGetInt, docID, uintptr(unsafe.Pointer(&key[0])), uintptr(len(key)), uintptr(unsafe.Pointer(i)))
		measure("FnArrayLen", FnArrayLen, uintptr(*arr), uintptr(unsafe.Pointer(n)), 0, 0)
		measure("FnPushString", FnPushString, uintptr(*arr), uintptr(unsafe.Pointer(&text[0])), uintptr(len(text)), 0)
		measure("FnPushString", FnPushString, uintptr(*arr), 0, 0, 0) // empty string
	}
	measure("FnAbiWord", FnAbiWord, 0, 0, 0, 0)

	for _, id := range ids {
		hybridge.CrossDeep1(FnRelease, uintptr(id))
	}
	hybridge.CrossDeep1(FnRelease, uintptr(*arr))

	for name, pr := range p {
		if pr.worst > pr.limit {
			t.Errorf("%s: measured stack high-water %d B exceeds its %d B guard class — descriptors.md's bound is false; route it through the DEFAULT (or DEEP) class",
				name, pr.worst, pr.limit)
		}
	}
}
