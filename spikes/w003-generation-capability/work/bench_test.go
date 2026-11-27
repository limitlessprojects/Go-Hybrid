package gencap

import (
	"strconv"
	"testing"
)

// H4: kill criterion 1 of #193 — median overhead of the runtime check on
// map-heavy and slice-heavy single-task paths must stay <= 5%.

const nKeys = 1 << 12

var keys = func() []string {
	k := make([]string, nKeys)
	for i := range k {
		k[i] = "key-" + strconv.Itoa(i)
	}
	return k
}()

var sinkInt int

// Map-heavy: word-count style increments.
func BenchmarkMapPlain(b *testing.B) {
	m := make(map[string]int, nKeys)
	for i := 0; i < b.N; i++ {
		m[keys[i&(nKeys-1)]]++
	}
	sinkInt = len(m)
}

// Unchecked: same wrapper indirection, no generation check. Separates the
// cost of the check from the cost of this prototype's wrapper header.
func BenchmarkMapUnchecked(b *testing.B) {
	x := Mint(make(map[string]int, nKeys))
	for i := 0; i < b.N; i++ {
		x.h.v[keys[i&(nKeys-1)]]++
	}
	sinkInt = len(x.h.v)
}

func BenchmarkMapPerAccess(b *testing.B) {
	x := Mint(make(map[string]int, nKeys))
	for i := 0; i < b.N; i++ {
		x.Get()[keys[i&(nKeys-1)]]++
	}
	sinkInt = len(x.Get())
}

// Slice-heavy: a tight index loop over 4096 ints per op. The length is kept
// opaque to the compiler in every variant (newSlice is not inlined), so no
// variant gets bounds-check elimination the others cannot.
const nSlice = 4096

//go:noinline
func newSlice() []int { return make([]int, nSlice) }

func BenchmarkSlicePlain(b *testing.B) {
	s := newSlice()
	for i := 0; i < b.N; i++ {
		for j := 0; j < nSlice; j++ {
			s[j] += j
		}
	}
	sinkInt = s[nSlice-1]
}

func BenchmarkSlicePerAccess(b *testing.B) {
	x := Mint(newSlice())
	for i := 0; i < b.N; i++ {
		for j := 0; j < nSlice; j++ {
			x.Get()[j] += j
		}
	}
	sinkInt = x.Get()[nSlice-1]
}

func BenchmarkSliceUnchecked(b *testing.B) {
	x := Mint(newSlice())
	for i := 0; i < b.N; i++ {
		for j := 0; j < nSlice; j++ {
			x.h.v[j] += j
		}
	}
	sinkInt = x.h.v[nSlice-1]
}

// Hoisted: one Get() per straight-line region with no intervening move/call.
func BenchmarkSliceHoisted(b *testing.B) {
	x := Mint(newSlice())
	for i := 0; i < b.N; i++ {
		s := x.Get()
		for j := 0; j < nSlice; j++ {
			s[j] += j
		}
	}
	sinkInt = x.Get()[nSlice-1]
}

// Boundary cost: one Move per op (the per-transfer price, not per-use).
func BenchmarkMove(b *testing.B) {
	x := Mint(map[string]int{})
	for i := 0; i < b.N; i++ {
		x = x.Move()
	}
	sinkInt = int(x.gen)
}

// Same-kernel pair: plain and hoisted call one shared loop body, so the only
// difference is the single Get() per op (removes code-layout effects from the
// comparison; see RESULTS.md H4).
//
//go:noinline
func addKernel(s []int) {
	for j := range s {
		s[j] += j
	}
}

func BenchmarkSliceKernelPlain(b *testing.B) {
	s := newSlice()
	for i := 0; i < b.N; i++ {
		addKernel(s)
	}
	sinkInt = s[nSlice-1]
}

func BenchmarkSliceKernelHoisted(b *testing.B) {
	x := Mint(newSlice())
	for i := 0; i < b.N; i++ {
		addKernel(x.Get())
	}
	sinkInt = x.Get()[nSlice-1]
}
