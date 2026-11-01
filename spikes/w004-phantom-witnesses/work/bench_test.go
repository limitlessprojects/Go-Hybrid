package w004

import "testing"

type bX struct{}

var sinkMap map[string]int

// P1 quantified: the witnessed path must allocate NOTHING and stay within
// noise of the plain baseline (mirrors #370's P1 so the two spikes compare).
func BenchmarkPlainMoveBaseline(b *testing.B) {
	m := map[string]int{"k": 1}
	b.ReportAllocs()
	for i := 0; i < b.N; i++ {
		x := m
		sinkMap = x
	}
}

func BenchmarkWitnessedMove(b *testing.B) {
	m := map[string]int{"k": 1}
	b.ReportAllocs()
	for i := 0; i < b.N; i++ {
		w := Live[bX]{}
		w2, x := hyMove(w, m)
		sinkMap = x
		_ = w2
	}
}
