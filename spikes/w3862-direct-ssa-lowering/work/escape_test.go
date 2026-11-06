package ssa

import (
	"testing"
)

// BenchmarkEscape_StackLocalBorrow simulates stack allocation of a 64-byte borrowed buffer with NOSPLIT leaf cert.
func BenchmarkEscape_StackLocalBorrow(b *testing.B) {
	b.ReportAllocs()
	for i := 0; i < b.N; i++ {
		// Stack-promoted local memory allocation (zero heap allocation)
		var buf [64]byte
		buf[0] = 0xAA
		buf[63] = 0xBB

		val := int64(buf[0]) + int64(buf[63])
		if val != 0xAA+0xBB {
			b.Fatalf("unexpected value: %d", val)
		}
	}
}

// BenchmarkEscape_HeapGlobalEscaped simulates plain-Go text compiler escaping the temporary buffer to heap.
func BenchmarkEscape_HeapGlobalEscaped(b *testing.B) {
	b.ReportAllocs()
	for i := 0; i < b.N; i++ {
		// Heap-promoted allocation through dynamic slice / make
		buf := make([]byte, 64)
		buf[0] = 0xAA
		buf[63] = 0xBB

		val := int64(buf[0]) + int64(buf[63])
		if val != 0xAA+0xBB {
			b.Fatalf("unexpected value: %d", val)
		}
	}
}
