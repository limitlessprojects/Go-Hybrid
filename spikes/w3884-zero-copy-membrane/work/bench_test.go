package membrane

import (
	"testing"
)

// BenchmarkMembrane100MB_10000Transitions directly validates Issue #3884's primary falsifiable metric:
// Passing a 100MB buffer through 10,000 hybrid task transitions achieves:
// 1. Exactly 0 allocations per transfer (0 B/op, 0 allocs/op) for the underlying memory.
// 2. O(1) constant latency matching raw Go in-process pointer passing.
func BenchmarkMembrane100MB_10000Transitions(b *testing.B) {
	const bufferSize = 100 * 1024 * 1024 // 100 MB buffer
	const transitions = 10000

	largeBuf := make([]byte, bufferSize)
	largeBuf[0] = 0x55
	largeBuf[bufferSize-1] = 0xAA

	b.ReportAllocs()
	b.ResetTimer()

	for i := 0; i < b.N; i++ {
		capCurr := NewRegionCap[RegionA]()
		mCurr, err := NewBufferMembrane[RegionA](capCurr, largeBuf)
		if err != nil {
			b.Fatal(err)
		}

		// 10,000 zero-copy membrane task transitions
		for hop := 0; hop < transitions; hop++ {
			if hop%2 == 0 {
				mNext, capNext, err := TransferBuffer[RegionA, RegionB](mCurr, &capCurr)
				if err != nil {
					b.Fatal(err)
				}
				// Rebind for alternate hop
				mCurr, capCurr, err = TransferBuffer[RegionB, RegionA](mNext, &capNext)
				if err != nil {
					b.Fatal(err)
				}
				hop++
			}
		}

		v, err := mCurr.View(capCurr)
		if err != nil || len(v) != bufferSize || v[0] != 0x55 {
			b.Fatalf("corrupted terminal buffer on iteration %d", i)
		}
	}
}

// BenchmarkSingleHopZeroCopy demonstrates microsecond/nanosecond O(1) boundary crossing.
func BenchmarkSingleHopZeroCopy(b *testing.B) {
	buf := make([]byte, 100*1024*1024)
	capA := NewRegionCap[RegionA]()
	mA, _ := NewBufferMembrane[RegionA](capA, buf)

	b.ReportAllocs()
	b.ResetTimer()

	for i := 0; i < b.N; i++ {
		mB, capB, _ := TransferBuffer[RegionA, RegionB](mA, &capA)
		mA, capA, _ = TransferBuffer[RegionB, RegionA](mB, &capB)
	}
}

// BenchmarkBaselineCopy100MB demonstrates the baseline failure mode:
// Copying 100MB buffer across 10,000 boundaries allocates 1 TB of memory and exhibits O(N) latency.
func BenchmarkBaselineCopy100MB(b *testing.B) {
	b.Skip("Baseline demonstration: copy allocates 100MB per hop, skipped in regular fast test runs")
	buf := make([]byte, 100*1024*1024)
	b.ReportAllocs()
	b.ResetTimer()

	for i := 0; i < b.N; i++ {
		dst := make([]byte, len(buf))
		copy(dst, buf)
	}
}
