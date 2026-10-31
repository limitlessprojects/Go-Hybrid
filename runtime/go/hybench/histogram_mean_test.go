package hybench

import (
	"math"
	"math/big"
	"testing"
)

func TestMeanDoesNotOverflowAggregate(t *testing.T) {
	cases := []struct {
		name    string
		samples []int64
	}{
		{"signed_sum_overflow", []int64{math.MaxInt64, math.MaxInt64}},
		{"unsigned_sum_overflow", []int64{math.MaxInt64, math.MaxInt64, math.MaxInt64}},
		{"mixed_magnitudes", []int64{0, 1, math.MaxInt64, math.MaxInt64, 7}},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			h := New()
			sum := new(big.Int)
			for _, sample := range tc.samples {
				h.Record(sample)
				sum.Add(sum, big.NewInt(sample))
			}
			want, _ := new(big.Rat).SetFrac(sum, big.NewInt(int64(len(tc.samples)))).Float64()
			if got := h.Mean(); math.Abs(got-want) > want*1e-15 {
				t.Fatalf("Mean() = %g, want %g for %v", got, want, tc.samples)
			}
			if h.Count() != int64(len(tc.samples)) {
				t.Fatalf("Count() = %d, want %d", h.Count(), len(tc.samples))
			}
		})
	}
}
