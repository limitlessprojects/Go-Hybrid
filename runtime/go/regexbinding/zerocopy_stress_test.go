package regexbinding

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

// TestCrossStringUnderPressure is spike7's soundness stress for the
// regex binding: pattern and text cross as raw {ptr, len} views of
// their own backing bytes (crossString), interleaved with runtime.GC()
// and forced stack growth, result-checked every iteration. See
// spikes/spike7-zerocopy-string/RESULTS.md.
func TestCrossStringUnderPressure(t *testing.T) {
	longPattern := "^" + strings.Repeat("a", 200) + "$"
	longText := strings.Repeat("a", 200)
	for i := 0; i < 300; i++ {
		ok, err := IsMatch("^h.llo$", "hello")
		if err != nil || !ok {
			t.Fatalf("iter %d: IsMatch(short) = %v, %v", i, ok, err)
		}
		ok, err = IsMatch(longPattern, longText)
		if err != nil || !ok {
			t.Fatalf("iter %d: IsMatch(long) = %v, %v", i, ok, err)
		}
		if !IsValid("[a-z]+") {
			t.Fatalf("iter %d: IsValid = false", i)
		}
		runtime.GC()
		stackSink += growStack(0)
	}
}
