package hybench

import "testing"

// Record instrumentation should stay small compared with the operation measured.
// Each case varies samples so this includes normal per-record exponent lookup.
func BenchmarkHistogramRecord(b *testing.B) {
	cases := []struct {
		name    string
		samples []int64
	}{
		{"nanosecond", []int64{3, 5, 7}},
		{"microsecond", []int64{900, 1000, 1100}},
		{"millisecond", []int64{900000, 1000000, 1100000}},
	}
	for _, tc := range cases {
		b.Run(tc.name, func(b *testing.B) {
			h := New()
			b.ReportAllocs()
			b.ResetTimer()
			for i := 0; i < b.N; i++ {
				h.Record(tc.samples[i%len(tc.samples)])
			}
			b.StopTimer()
			if h.Count() != int64(b.N) {
				b.Fatalf("recorded %d samples, want %d", h.Count(), b.N)
			}
		})
	}
}
