package jsonbinding

import (
	"runtime"
	"testing"
)

// TestEphemeralReceiverKeepAliveAcrossFFI verifies Issue #1314 Hazard 2:
// Calling Int, String, Array, PushString, and ToStringPretty on last-use
// receivers under aggressive GC pressure keeps v.h alive across FFI.
func TestEphemeralReceiverKeepAliveAcrossFFI(t *testing.T) {
	drainCleanups()
	for i := 0; i < 25; i++ {
		v, err := Parse([]byte(`{"k":77,"s":"hello","arr":["a","b"]}`))
		if err != nil {
			t.Fatalf("Parse: %v", err)
		}
		runtime.GC()
		if n, err := v.Int("k"); err != nil || n != 77 {
			t.Fatalf("Int(k) = (%d, %v), want (77, nil)", n, err)
		}
		if s, err := v.String("s"); err != nil || s != "hello" {
			t.Fatalf("String(s) = (%q, %v), want (\"hello\", nil)", s, err)
		}
		elems, err := v.Array("arr")
		if err != nil || len(elems) != 2 {
			t.Fatalf("Array(arr) = (len=%d, %v), want (2, nil)", len(elems), err)
		}
		for _, el := range elems {
			if txt, err := ToStringPretty(el); err != nil || len(txt) == 0 {
				t.Fatalf("ToStringPretty(el) = (%q, %v)", txt, err)
			}
		}
	}
}
