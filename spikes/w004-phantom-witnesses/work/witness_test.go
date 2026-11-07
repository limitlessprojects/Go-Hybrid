package witness

import (
	"testing"
)

// Positive control: valid use under hyLive compiles and executes with 0 cost
func TestValidOwnershipFlow(t *testing.T) {
	var capX hyLive[RootA]
	hyUse(capX)
	hyRequireLive(capX)

	// Transfer opens continuation
	{
		var capX hyMoved
		_ = capX
		// In a valid program, no hyUse(capX) occurs here
	}
}

// Benchmark verifying zero allocations and zero runtime overhead
func BenchmarkWitnessOverhead(b *testing.B) {
	b.ReportAllocs()
	var capX hyLive[RootA]

	for i := 0; i < b.N; i++ {
		hyUse(capX)
		hyRequireLive(capX)
	}
}

func BenchmarkLoopBackedgeWitness(b *testing.B) {
	b.ReportAllocs()
	var capX hyLive[RootA]

	for i := 0; i < b.N; i++ {
		// Static backedge requirement:
		var _ hyLive[RootA] = capX
	}
}
