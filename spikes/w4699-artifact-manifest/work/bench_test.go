package w4699artifactmanifest

import "testing"

// BenchmarkManifestBuildFIFO measures the cost of building the canonical
// 7-node binary-tree manifest under the FIFO policy.
func BenchmarkManifestBuildFIFO(b *testing.B) {
	for i := 0; i < b.N; i++ {
		c := NewCoordinator(binaryTreeWorker, Budget{MaxTasks: 64, MaxBytes: 1 << 16})
		c.SetRootValue(Value{'1'})
		if _, err := c.Run(FIFO); err != nil {
			b.Fatal(err)
		}
	}
}

// BenchmarkManifestEncode measures the cost of canonical manifest encoding.
func BenchmarkManifestEncode(b *testing.B) {
	c := NewCoordinator(binaryTreeWorker, Budget{MaxTasks: 64, MaxBytes: 1 << 16})
	c.SetRootValue(Value{'1'})
	m, err := c.Run(FIFO)
	if err != nil {
		b.Fatal(err)
	}
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		_ = m.Encode()
	}
}

// BenchmarkManifestRandomSchedule measures the cost under the Random policy.
func BenchmarkManifestRandomSchedule(b *testing.B) {
	for i := 0; i < b.N; i++ {
		c := NewCoordinator(binaryTreeWorker, Budget{MaxTasks: 64, MaxBytes: 1 << 16})
		c.SetRootValue(Value{'1'})
		c.SetSeed(uint64(i) + 1)
		if _, err := c.Run(Random); err != nil {
			b.Fatal(err)
		}
	}
}

var _ = testing.Short