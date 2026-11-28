package mesh

import "testing"

var benchPoints = [3]Point{
	{X: 3, Y: -5},
	{X: 3 + (1 << 27), Y: -5 + (1 << 27) - 1},
	{X: 3 + (1 << 27) + 1, Y: -5 + (1 << 27)},
}

func benchMesh(b *testing.B) *Mesh {
	b.Helper()
	m, err := New(1, benchPoints[0], benchPoints[1], benchPoints[2])
	if err != nil {
		b.Fatalf("New: %v", err)
	}
	return m
}

// BenchmarkClassifyHard is the exact path: the unit-determinant family forces
// the 128-bit fallback every time.
func BenchmarkClassifyHard(b *testing.B) {
	b.ReportAllocs()
	for i := 0; i < b.N; i++ {
		_, _ = classify(benchPoints[0], benchPoints[1], benchPoints[2])
	}
}

// BenchmarkSignNaive is the same input through the obvious float64 code.
func BenchmarkSignNaive(b *testing.B) {
	b.ReportAllocs()
	for i := 0; i < b.N; i++ {
		_ = Sign(benchPoints[0], benchPoints[1], benchPoints[2])
	}
}

// BenchmarkObserve is mint cost: predicate + 48-byte digest + binding.
func BenchmarkObserve(b *testing.B) {
	m := benchMesh(b)
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		_, _ = m.Observe(Triple{0, 1, 2})
	}
}

// BenchmarkInsertDigest is the full gate: four checks plus the digest.
func BenchmarkInsertDigest(b *testing.B) {
	m := benchMesh(b)
	w, _ := m.Observe(Triple{0, 1, 2})
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		m.epoch = w.epoch // isolate the gate from the epoch bump
		_, _ = m.Insert(Triple{0, 1, 2}, w)
	}
}

// BenchmarkInsertEpochOnly prices the digest: same gate without it.
func BenchmarkInsertEpochOnly(b *testing.B) {
	m := benchMesh(b)
	w, _ := m.Observe(Triple{0, 1, 2})
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		m.epoch = w.epoch
		_, _ = m.insertEpochOnly(Triple{0, 1, 2}, w)
	}
}

// BenchmarkWriteNoCheck is the floor: what an unguarded write path costs.
func BenchmarkWriteNoCheck(b *testing.B) {
	m := benchMesh(b)
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		m.epoch++
	}
}
