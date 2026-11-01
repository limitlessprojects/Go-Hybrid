// Package hybench — the bench harnesses' shared measurement utilities
// (PLAN row 18: the community benchmark report found ms-clock harnesses too
// coarse for µs alloc tails, and unmatched queue shapes manufactured fake
// gaps — "the matched-shape control dissolved an 8× 'gap' into parity").
//
// HDR-style histogram: the same log-linear bucketing the HdrHistogram
// project uses — base-2 exponent buckets with 1/1024 linear sub-buckets
// inside each — so a recorded value keeps ~0.1% relative resolution across
// the whole range at a flat memory cost (~64Ki counters worst case for the
// u64 range). Record is a handful of arithmetic ops; no locks (a bench
// goroutine records, the main goroutine reads after a barrier).
//
// Timer honesty at ns scale: per-call time.Now() costs ~20-30 ns, which
// would DOMINATE a ≤5 ns crossing. The rule this package pins:
//   - batched ops (fast paths, ≤ ~100 ns/op): record WINDOW means
//     (one sample per K calls, each sample = window/K) — the distribution
//     is over window means, which is what a fixed-rate consumer sees;
//   - slow ops (µs and up): record every call directly.
package hybench

import (
	"fmt"
	"math"
)

// SubHalf is the sub-bucket half-count: 2^10 linear sub-buckets per
// exponent bucket ⇒ ~0.1% resolution (HDR's default).
const SubHalf = 1024

// Histogram is an HDR-style fixed-memory latency histogram (ns units).
type Histogram struct {
	counts   [64 * SubHalf]int64 // [exponent][sub-bucket]
	subMask  int64
	loShift  [64]int // per-exponent right-shift keeping the top sub-bucket bit
	maxVal   int64
	minVal   int64
	count    int64
	total    int64 // sum of recorded values (for the mean)
	overflow int64
}

// New allocates a histogram. The zero value is also lazily initialized on
// first Record.
func New() *Histogram {
	h := &Histogram{}
	h.ensureInit()
	return h
}

func (h *Histogram) ensureInit() {
	if h.subMask != 0 {
		return
	}
	h.subMask = SubHalf - 1
	h.minVal = math.MaxInt64
	for e := 0; e < 64; e++ {
		// exponent e covers magnitudes [2^e, 2^(e+1)); the sub-bucket
		// index keeps the leading 1 implicit: shift so it drops off
		// and 10 fraction bits remain.
		h.loShift[e] = e - 10
		if h.loShift[e] < 0 {
			h.loShift[e] = 0
		}
	}
}

// Record one latency sample (ns). Negative values are ignored (timer
// anomalies); zero is recorded (it is real).
//
// Bucketing regimes (HDR's log-linear scheme):
//   - e < 10 (values < 1024): loShift is 0, so the sub-bucket index IS the
//     value — every integer gets its own bucket (exact).
//   - e >= 10: the value's leading bit falls outside the 10-bit sub-bucket
//     mask, so sub = (ns >> shift) & 1023 with the leading bit IMPLICIT;
//     reconstruction re-adds it (see ValueAt).
func (h *Histogram) Record(ns int64) {
	if ns < 0 {
		return
	}
	h.ensureInit()
	h.count++
	h.total += ns
	if ns > h.maxVal {
		h.maxVal = ns
	}
	if ns < h.minVal {
		h.minVal = ns
	}
	if ns == 0 {
		h.counts[0]++ // the dedicated zero bucket (e=0, sub=0)
		return
	}
	e := 63 - leadingZeros(uint64(ns)) // magnitude exponent
	if e >= 64 {
		h.overflow++
		return
	}
	sub := (ns >> uint(h.loShift[e])) & h.subMask
	h.counts[e*SubHalf+int(sub)]++
}

// Count is the number of recorded samples.
func (h *Histogram) Count() int64 { return h.count }

// Mean is the sample mean in ns (0 when empty).
func (h *Histogram) Mean() float64 {
	if h.count == 0 {
		return 0
	}
	return float64(h.total) / float64(h.count)
}

// ValueAt returns the ns value the given fraction (0..1) of samples fall
// at or below (the upper bound of the bucket that crosses the rank, clamped
// to [minVal, maxVal]).
func (h *Histogram) ValueAt(fraction float64) int64 {
	if h.count == 0 || math.IsNaN(fraction) {
		return 0
	}
	if fraction <= 0 {
		return h.minVal
	}
	if fraction >= 1 {
		return h.maxVal
	}
	rank := int64(fraction*float64(h.count) + 0.5)
	if rank < 1 {
		rank = 1
	}
	if rank > h.count {
		rank = h.count
	}
	var seen int64
	for e := 0; e < 64; e++ {
		for sub := 0; sub < SubHalf; sub++ {
			seen += h.counts[e*SubHalf+sub]
			if seen >= rank {
				var v int64
				if e < 10 {
					// exact regime: the sub-bucket index IS the value
					// (sub 0 is the dedicated zero bucket)
					v = int64(sub)
				} else {
					// implicit-leading-bit regime: re-add SubHalf in uint64
					// so e=62, sub=1023 does not signed-overflow int64.
					shift := uint(h.loShift[e])
					u := (uint64(SubHalf+sub+1) << shift) - 1
					if u > uint64(math.MaxInt64) {
						v = math.MaxInt64
					} else {
						v = int64(u)
					}
				}
				if v < h.minVal {
					return h.minVal
				}
				if v > h.maxVal {
					return h.maxVal
				}
				return v
			}
		}
	}
	return h.maxVal
}

// P50, P90, P99 are the common tails.
func (h *Histogram) P50() int64 { return h.ValueAt(0.50) }
func (h *Histogram) P90() int64 { return h.ValueAt(0.90) }
func (h *Histogram) P99() int64 { return h.ValueAt(0.99) }

// Summary renders one machine-parseable line:
// `p50=Xns p90=Xns p99=Xns mean=X.Xns n=N`.
func (h *Histogram) Summary() string {
	return fmt.Sprintf("p50=%dns p90=%dns p99=%dns mean=%.1fns n=%d",
		h.P50(), h.P90(), h.P99(), h.Mean(), h.count)
}

// SummaryPrefixed renders `<name> <summary>` — the machine-parseable
// shape the bench harnesses emit.
func (h *Histogram) SummaryPrefixed(name string) string {
	return name + " " + h.Summary()
}

func leadingZeros(x uint64) int {
	if x == 0 {
		return 64
	}
	n := 0
	for i := 63; i >= 0; i-- {
		if x&(1<<uint(i)) != 0 {
			break
		}
		n++
	}
	return n
}
