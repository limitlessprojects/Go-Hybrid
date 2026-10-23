package main

import (
	"math"
	"sort"
	"time"
)

// Stats is the latency summary for one engine's sample set. Percentiles are
// nearest-rank on the sorted samples: no interpolation, so every number
// reported is a measurement that actually happened.
type Stats struct {
	N    int     `json:"n"`
	P50  float64 `json:"p50_ms"`
	P90  float64 `json:"p90_ms"`
	P99  float64 `json:"p99_ms"`
	Min  float64 `json:"min_ms"`
	Max  float64 `json:"max_ms"`
	Mean float64 `json:"mean_ms"`
}

func Summarize(samples []time.Duration) Stats {
	if len(samples) == 0 {
		return Stats{}
	}
	sorted := make([]time.Duration, len(samples))
	copy(sorted, samples)
	sort.Slice(sorted, func(i, j int) bool { return sorted[i] < sorted[j] })
	var total time.Duration
	for _, s := range sorted {
		total += s
	}
	return Stats{
		N:    len(sorted),
		P50:  ms(percentile(sorted, 50)),
		P90:  ms(percentile(sorted, 90)),
		P99:  ms(percentile(sorted, 99)),
		Min:  ms(sorted[0]),
		Max:  ms(sorted[len(sorted)-1]),
		Mean: ms(total / time.Duration(len(sorted))),
	}
}

// percentile returns the nearest-rank p-th percentile of an ascending slice.
func percentile(sorted []time.Duration, p int) time.Duration {
	if len(sorted) == 0 {
		return 0
	}
	if p <= 0 {
		return sorted[0]
	}
	if p >= 100 {
		return sorted[len(sorted)-1]
	}
	rank := int(math.Ceil(float64(p) / 100 * float64(len(sorted))))
	if rank < 1 {
		rank = 1
	}
	if rank > len(sorted) {
		rank = len(sorted)
	}
	return sorted[rank-1]
}

func ms(d time.Duration) float64 {
	return math.Round(float64(d)/float64(time.Microsecond)) / 1000
}
