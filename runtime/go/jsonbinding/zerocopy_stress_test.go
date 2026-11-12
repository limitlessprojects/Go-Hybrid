package jsonbinding

import (
	"runtime"
	"strings"
	"testing"
)

var stackSink int

// growStack forces this goroutine's stack to grow (and later shrink) by
// recursing with a non-trivial frame, so stack copying interleaves with
// the crossings in TestCrossStringUnderPressure.
//
//go:noinline
func growStack(depth int) int {
	var frame [256]byte
	frame[depth%256] = byte(depth)
	if depth >= 1200 {
		return int(frame[0])
	}
	return growStack(depth+1) + int(frame[depth%256])
}

// TestCrossStringUnderPressure is spike7's soundness stress: keyed calls
// whose key strings cross as raw {ptr, len} views of their own backing
// bytes (crossString), interleaved with runtime.GC() and forced stack
// growth — the conditions under which a raw pointer into a moved string
// would dangle. Every call is result-checked; a stale pointer shows up
// as a wrong value or a miss. Key shapes covered: a literal (rodata
// backing), short heap strings, a substring of one large heap string,
// and a 300-byte key. Deliberately stack-resident argument strings could
// not be constructed from Go source for this test (string conversions
// escape to the heap); see spikes/spike7-zerocopy-string/RESULTS.md.
func TestCrossStringUnderPressure(t *testing.T) {
	big := strings.Repeat("abcdefgh", 512)
	subKey := big[100:200]
	longKey := strings.Repeat("L", 300)

	doc, err := Parse([]byte(`{"k": 42, "name": "hy"}`))
	if err != nil {
		t.Fatalf("Parse: %v", err)
	}
	for _, kv := range []struct {
		key string
		val int
	}{{"mid", 7}, {subKey, 9}, {longKey, 11}} {
		if err := doc.Set(kv.key, kv.val); err != nil {
			t.Fatalf("Set(key len %d): %v", len(kv.key), err)
		}
	}

	cases := []struct {
		key  string
		want int64
	}{{"k", 42}, {"mid", 7}, {subKey, 9}, {longKey, 11}}

	for i := 0; i < 2000; i++ {
		for _, tc := range cases {
			got, err := doc.Int(tc.key)
			if err != nil || got != tc.want {
				t.Fatalf("iter %d: Int(key len %d) = %d, %v (want %d)", i, len(tc.key), got, err, tc.want)
			}
		}
		if s, err := doc.String("name"); err != nil || s != "hy" {
			t.Fatalf("iter %d: String(name) = %q, %v", i, s, err)
		}
		arr := NewArray()
		if err := arr.PushString("payload"); err != nil {
			t.Fatalf("iter %d: PushString: %v", i, err)
		}
		runtime.GC()
		stackSink += growStack(0)
	}
}
