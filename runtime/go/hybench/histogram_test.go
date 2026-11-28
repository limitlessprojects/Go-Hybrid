package hybench

import (
	"math"
	"testing"
)

func TestRecordsAndRanks(t *testing.T) {
	h := New()
	for i := 1; i <= 1000; i++ {
		h.Record(int64(i)) // 1..1000 ns
	}
	if h.Count() != 1000 {
		t.Fatalf("count = %d", h.Count())
	}
	p50 := h.P50()
	p99 := h.P99()
	if p50 < 490 || p50 > 520 {
		t.Fatalf("p50 out of range: %d (want ~500)", p50)
	}
	if p99 < 985 || p99 > 1005 {
		t.Fatalf("p99 out of range: %d (want ~1000)", p99)
	}
	if h.Mean() < 499 || h.Mean() > 501 {
		t.Fatalf("mean out of range: %.1f", h.Mean())
	}
}

func TestResolutionAcrossMagnitudes(t *testing.T) {
	// The HDR property: ~0.1% relative resolution at 1ns AND at 1s.
	h := New()
	h.Record(1)
	h.Record(2)
	h.Record(1_000_000_000) // 1 s
	if h.P99() < 1_000_000_000 {
		t.Fatalf("p99 lost the big sample: %d", h.P99())
	}
	h2 := New()
	for i := 0; i < 100; i++ {
		h2.Record(1000) // 1µs exactly
	}
	h2.Record(1002) // +0.2% — must land in a DIFFERENT sub-bucket
	h2.Record(1002)
	// 102 samples: rank(0.99) = 101 → the 1002 bucket
	if h2.P99() < 1001 {
		t.Fatalf("p99 could not distinguish +0.2%%: %d", h2.P99())
	}
}

func TestWindowMeansAreWhatWindowsSee(t *testing.T) {
	// The timer-honesty rule: for a 3 ns op, record one sample per 10k
	// window (the window mean). The histogram's p50 must sit at the
	// window mean, NOT near zero and NOT inflated by timer cost.
	h := New()
	for w := 0; w < 100; w++ {
		h.Record(3000) // 10_000 calls × 0.3 ns ... use 3µs window mean
	}
	if h.P50() < 2500 || h.P50() > 3500 {
		t.Fatalf("window-mean p50 out of range: %d", h.P50())
	}
}

func TestEmptyAndEdges(t *testing.T) {
	h := New()
	if h.P50() != 0 || h.Mean() != 0 || h.Count() != 0 {
		t.Fatal("empty histogram must be zero-valued")
	}
	h.Record(-5) // ignored
	if h.Count() != 0 {
		t.Fatal("negative samples are ignored")
	}
	h.Record(0)
	if h.P50() != 0 {
		t.Fatalf("a recorded zero is real: %d", h.P50())
	}
}

func TestSummaryShape(t *testing.T) {
	h := New()
	h.Record(10)
	line := h.SummaryPrefixed("workload-a")
	want := "workload-a p50="
	if len(line) < len(want) || line[:len(want)] != want {
		t.Fatalf("summary shape drifted: %q", line)
	}
}

func TestHighMagnitudeMinNaNAndBucketClamp(t *testing.T) {
	h := New()
	high := int64(1)<<62 + 12345
	h.Record(high)
	if got := h.ValueAt(0); got != high {
		t.Fatalf("ValueAt(0) for sample >= 1<<62 = %d, want %d", got, high)
	}
	if got := h.ValueAt(math.NaN()); got != 0 {
		t.Fatalf("ValueAt(NaN) = %d, want 0", got)
	}

	u := New()
	for i := 0; i < 100; i++ {
		u.Record(3000)
	}
	if u.P50() != 3000 || u.P99() != 3000 || u.ValueAt(1.0) != 3000 {
		t.Fatalf("uniform samples above 1024 exceeded maxVal: p50=%d p99=%d max=%d",
			u.P50(), u.P99(), u.ValueAt(1.0))
	}

	top := New()
	top.Record(math.MaxInt64)
	if top.P50() != math.MaxInt64 || top.P99() != math.MaxInt64 {
		t.Fatalf("MaxInt64 bucket reconstruction drifted: p50=%d p99=%d", top.P50(), top.P99())
	}
}

func TestValueAtRankCoverage(t *testing.T) {
	// Reported tail cases: rounding to nearest integer can drop below the requested fraction.
	h90 := New()
	for i := 0; i < 5; i++ {
		h90.Record(10)
	}
	h90.Record(1000)
	if got := h90.P90(); got != 1000 {
		t.Fatalf("P90(5x10, 1x1000) = %d, want 1000", got)
	}

	h99 := New()
	for i := 0; i < 59; i++ {
		h99.Record(10)
	}
	h99.Record(1000)
	if got := h99.P99(); got != 1000 {
		t.Fatalf("P99(59x10, 1x1000) = %d, want 1000", got)
	}

	// Exact and near-exact boundaries over 100 distinct samples (1..100).
	h100 := New()
	for i := int64(1); i <= 100; i++ {
		h100.Record(i)
	}
	if got := h100.ValueAt(0.14); got != 14 {
		t.Fatalf("ValueAt(0.14) = %d, want 14", got)
	}
	if got := h100.ValueAt(math.Nextafter(0.14, 1)); got != 15 {
		t.Fatalf("ValueAt(Nextafter(0.14, 1)) = %d, want 15", got)
	}
	if got := h100.ValueAt(0.29); got != 29 {
		t.Fatalf("ValueAt(0.29) = %d, want 29", got)
	}
	if got := h100.P50(); got != 50 {
		t.Fatalf("P50(1..100) = %d, want 50", got)
	}
	if got := h100.P90(); got != 90 {
		t.Fatalf("P90(1..100) = %d, want 90", got)
	}
	if got := h100.P99(); got != 99 {
		t.Fatalf("P99(1..100) = %d, want 99", got)
	}
	if got := h100.ValueAt(math.Nextafter(0, 1)); got != 1 {
		t.Fatalf("ValueAt(Nextafter(0, 1)) = %d, want 1", got)
	}
}
