package noheap

import "testing"

func BenchmarkNaiveEmit_64Slots(b *testing.B) {
	for i := 0; i < b.N; i++ {
		_ = NaiveEmitSensorWindow(64, int64(i))
	}
}

func BenchmarkStackCertified_64Slots(b *testing.B) {
	tok := SynthesizeNoHeapToken()
	for i := 0; i < b.N; i++ {
		_ = StackCertifiedSensorWindow(tok, 64, int64(i))
	}
}
