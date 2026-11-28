// Command measure reproduces every number quoted in RESULTS.md.
package main

import (
	"fmt"
	"sort"
	"time"

	seek "github.com/limitlessprojects/spike-w4593-seek"
)

func stage32() seek.Stage {
	return seek.Stage{
		Name: "3to2",
		In:   3,
		Out:  2,
		Deps: [][]int{{0, 2}, {1}},
		Eval: func(f []uint32) uint32 {
			if len(f) == 2 {
				return f[0] + f[1]
			}
			return f[0]
		},
	}
}

func main() {
	c, err := seek.Compile(seek.Pipeline{Stages: []seek.Stage{stage32(), stage32()}})
	if err != nil {
		panic(err)
	}
	const n = 2_250_009
	words := make([]uint32, n)
	for i := range words {
		words[i] = uint32(i*3 + 1)
	}
	outLen, err := c.OutLen(n)
	if err != nil {
		panic(err)
	}
	fmt.Printf("source words=%d  outputs=%d\n\n", n, outLen)

	// ---- isolation: one late coordinate --------------------------------
	for _, k := range []int64{1_000_000, 1_000_001} {
		s := seek.NewSnapshot(words)
		v, err := c.At(s, k)
		if err != nil {
			panic(err)
		}
		fmt.Printf("At(%d) = %-12d reads=%-3d indices=%v\n", k, v, s.Reads(), s.Indices())
	}

	// ---- contiguous window ---------------------------------------------
	const win = 1000
	const winStart = 999_000
	unique := make(map[int]struct{}, 4096)
	totalReads := 0
	start := time.Now()
	for k := int64(winStart); k < int64(winStart+win); k++ {
		s := seek.NewSnapshot(words)
		if _, err := c.At(s, k); err != nil {
			panic(err)
		}
		totalReads += s.Reads()
		for _, i := range s.Indices() {
			unique[i] = struct{}{}
		}
	}
	winElapsed := time.Since(start)
	keys := make([]int, 0, len(unique))
	for i := range unique {
		keys = append(keys, i)
	}
	sort.Ints(keys)
	span := keys[len(keys)-1] - keys[0] + 1
	fmt.Printf("\nwindow: %d outputs [%d,%d)\n", win, winStart, winStart+win)
	fmt.Printf("  total source reads   = %d\n", totalReads)
	fmt.Printf("  distinct source words= %d  (span %d words, density %.2f%%)\n",
		len(unique), span, 100*float64(len(unique))/float64(span))
	fmt.Printf("  wall time            = %v  (%.1f us/output)\n",
		winElapsed.Round(time.Microsecond), float64(winElapsed.Microseconds())/win)

	// ---- scattered coordinates: the locality cost ----------------------
	scatter := make(map[int]struct{}, 8192)
	scatterReads := 0
	rng := uint64(0x9E3779B97F4A7C15)
	for i := 0; i < win; i++ {
		rng ^= rng << 13
		rng ^= rng >> 7
		rng ^= rng << 17
		k := int64(rng % uint64(outLen))
		s := seek.NewSnapshot(words)
		if _, err := c.At(s, k); err != nil {
			panic(err)
		}
		scatterReads += s.Reads()
		for _, idx := range s.Indices() {
			scatter[idx] = struct{}{}
		}
	}
	skeys := make([]int, 0, len(scatter))
	for i := range scatter {
		skeys = append(skeys, i)
	}
	sort.Ints(skeys)
	fmt.Printf("\nscatter: %d random coordinates over [0,%d)\n", win, outLen)
	fmt.Printf("  total source reads    = %d\n", scatterReads)
	fmt.Printf("  distinct source words = %d over a span of %d (density %.2f%%)\n",
		len(scatter), skeys[len(skeys)-1]-skeys[0]+1,
		100*float64(len(scatter))/float64(skeys[len(skeys)-1]-skeys[0]+1))
	// ---- dense baseline ------------------------------------------------
	start = time.Now()
	dense, err := c.Dense(words)
	denseElapsed := time.Since(start)
	if err != nil {
		panic(err)
	}
	fmt.Printf("\ndense: %d outputs in %v (%.0f ns/output)\n",
		len(dense), denseElapsed.Round(time.Microsecond),
		float64(denseElapsed.Nanoseconds())/float64(len(dense)))
	fmt.Printf("dense must touch every stage input: %d + %d + %d words\n",
		n, n/3*2, (n/3*2)/3*2)

	// ---- single-coordinate latency (amortised over many calls) ---------
	const calls = 200_000
	start = time.Now()
	var sink uint32
	for i := 0; i < calls; i++ {
		k := int64(800_000 + i%501)
		s := seek.NewSnapshot(words)
		v, err := c.At(s, k)
		if err != nil {
			panic(err)
		}
		sink += v
	}
	perCall := time.Since(start) / calls
	fmt.Printf("\nAt() amortised over %d calls: %v/call (sink=%d)\n", calls, perCall.Round(time.Nanosecond), sink)
	// ---- sequential prefix read for the same coordinate ----------------
	prefixReads := int64(1_000_000)/1 + 0
	_ = prefixReads
	fmt.Printf("\nfor comparison, a forward evaluator needs the whole prefix: %d source words\n", n)
}
