package regexbinding

import "testing"

// BenchmarkIsMatch: one pattern+text crossing end to end (spike7
// zerocopy-string measurement instrument). The Rust side recompiles the
// pattern per call, so ns/op is dominated by compilation; the Go-side
// allocs/op isolates the string arguments' crossing cost.
func BenchmarkIsMatch(b *testing.B) {
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		ok, err := IsMatch("^h.llo$", "hello")
		if err != nil || !ok {
			b.Fatalf("IsMatch = %v, %v", ok, err)
		}
	}
}
