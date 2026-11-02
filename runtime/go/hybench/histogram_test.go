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

func TestValueAtNumericBoundaries(t *testing.T) {
	nanHistogram := New()
	nanHistogram.Record(42)
	if got := nanHistogram.ValueAt(math.NaN()); got != 0 {
		t.Fatalf("ValueAt(NaN) = %d, want 0", got)
	}

	const maxInt64 = int64(1<<63 - 1)
	maxHistogram := New()
	maxHistogram.Record(maxInt64)
	if got := maxHistogram.ValueAt(0); got != maxInt64 {
		t.Fatalf("ValueAt(0) for MaxInt64 sample = %d, want %d", got, maxInt64)
	}

	const sample = 3000
	sameValueHistogram := New()
	for i := 0; i < 100; i++ {
		sameValueHistogram.Record(sample)
	}
	if got := sameValueHistogram.P50(); got > sameValueHistogram.ValueAt(1) {
		t.Fatalf("P50() = %d exceeds ValueAt(1) = %d", got, sameValueHistogram.ValueAt(1))
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
