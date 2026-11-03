package jsonbinding

import (
	"fmt"
	"strings"
	"testing"
)

// BenchmarkGetIntByKey: one keyed scalar read end to end (spike7
// zerocopy-string measurement instrument — the key argument's crossing
// cost is the variable; everything else in the call is held constant).
func BenchmarkGetIntByKey(b *testing.B) {
	doc, err := Parse([]byte(`{"k": 42}`))
	if err != nil {
		b.Fatalf("Parse: %v", err)
	}
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		n, err := doc.Int("k")
		if err != nil || n != 42 {
			b.Fatalf("Int(k) = %d, %v", n, err)
		}
	}
}

// BenchmarkGetIntByLongKey: same read with a 64-byte key, where the key
// argument's O(len) crossing cost (if any) is no longer negligible.
func BenchmarkGetIntByLongKey(b *testing.B) {
	key := strings.Repeat("k", 64)
	doc, err := Parse([]byte(fmt.Sprintf(`{%q: 7}`, key)))
	if err != nil {
		b.Fatalf("Parse: %v", err)
	}
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		n, err := doc.Int(key)
		if err != nil || n != 7 {
			b.Fatalf("Int(long key) = %d, %v", n, err)
		}
	}
}
