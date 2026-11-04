package hybench_test

import (
	"math"
	"testing"

	"hybrid.program/hybench"
)

func TestSingleSamplePercentilesNeverExceedMaxVal(t *testing.T) {
	// A sample in the e >= 10 regime at the bottom of its sub-bucket
	// (e.g. 2048: e=11, shift=1, sub=0) has bucket upper bound 2049,
	// which previously caused P50()/P99() (2049) to exceed ValueAt(1.0) (2048).
	for _, sample := range []int64{1024, 2048, 4096, 1_000_000} {
		h := hybench.New()
		h.Record(sample)
		if got := h.ValueAt(0); got != sample {
			t.Fatalf("sample %d: ValueAt(0) = %d, want %d", sample, got, sample)
		}
		if got := h.P50(); got != sample {
			t.Fatalf("sample %d: P50() = %d, want %d", sample, got, sample)
		}
		if got := h.P90(); got != sample {
			t.Fatalf("sample %d: P90() = %d, want %d", sample, got, sample)
		}
		if got := h.P99(); got != sample {
			t.Fatalf("sample %d: P99() = %d, want %d", sample, got, sample)
		}
		if got := h.ValueAt(1.0); got != sample {
			t.Fatalf("sample %d: ValueAt(1.0) = %d, want %d", sample, got, sample)
		}
	}
}

func TestHighMagnitudeMinValAndNoSignedOverflow(t *testing.T) {
	// Samples >= 1<<62 previously left minVal at 1<<62 - 1, and e=62, sub=1023
	// signed-overflowed int64 during bucket reconstruction.
	h := hybench.New()
	const high = int64(1) << 62
	h.Record(high)
	if got := h.ValueAt(0); got != high {
		t.Fatalf("ValueAt(0) = %d, want %d", got, high)
	}
	if got := h.P50(); got != high {
		t.Fatalf("P50() = %d, want %d", got, high)
	}

	hMax := hybench.New()
	hMax.Record(math.MaxInt64)
	if got := hMax.P50(); got <= 0 || got != math.MaxInt64 {
		t.Fatalf("MaxInt64 P50() = %d, want %d", got, int64(math.MaxInt64))
	}
}

func TestNaNFractionAndZeroValueHistogram(t *testing.T) {
	var h hybench.Histogram
	h.Record(2048)
	if got := h.ValueAt(math.NaN()); got != 0 {
		t.Fatalf("ValueAt(NaN) = %d, want 0", got)
	}
	if got := h.P50(); got != 2048 {
		t.Fatalf("zero-value Histogram P50() = %d, want 2048", got)
	}
}
