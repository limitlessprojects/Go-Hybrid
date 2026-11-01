package main

import (
	"encoding/binary"
	"testing"
)

// NaiveHeapKernel allocates on the heap dynamically
func NaiveHeapKernel(n int) int {
	buf := make([]byte, n)
	binary.LittleEndian.PutUint32(buf[0:4], 0x12345678)
	return int(buf[0])
}

// CertifiedStackKernel simulates Tier-0 stack-array lowering (var _hyStack [K]T)
// It performs frame-bounded operations with zero heap allocation.
//go:noinline
func CertifiedStackKernel(n int) int {
	var _hyStack [1024]byte
	if n > 1024 {
		return -1
	}
	buf := _hyStack[:n:1024]
	binary.LittleEndian.PutUint32(buf[0:4], 0x12345678)
	return int(buf[0])
}

// TestFalsifierP1 verifies that CertifiedStackKernel incurs strictly 0 allocations per run.
func TestFalsifierP1(t *testing.T) {
	allocs := testing.AllocsPerRun(1000, func() {
		res := CertifiedStackKernel(128)
		if res != 0x78 {
			t.Fatalf("unexpected result: got %x, want 0x78", res)
		}
	})

	if allocs > 0 {
		t.Fatalf("FALSIFIED (P1): CertifiedStackKernel allocated %f allocs/op, expected 0", allocs)
	}
}

func main() {
	res := CertifiedStackKernel(128)
	if res != 0x78 {
		panic("Self-check failed")
	}
	println("W-024 Falsifier P1: Self-check PASS (result == 0x78)")
}
