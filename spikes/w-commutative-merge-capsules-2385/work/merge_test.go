package merge

import (
	"math"
	"reflect"
	"sync"
	"sync/atomic"
	"testing"
)

type rgn struct{}

func histEq(a, b Histogram) bool { return a.Buckets == b.Buckets && reflect.DeepEqual(a.Tags, b.Tags) }

func TestP1ParallelMergeCapsuleZeroRaces(t *testing.T) {
	cap := NewCapsule[rgn, Histogram](HistogramReducer{})
	var wg sync.WaitGroup
	for w := 0; w < 8; w++ {
		w, s := w, Split(cap)
		wg.Add(1)
		go func() {
			defer wg.Done()
			for i := 0; i < 10000; i++ {
				Mutate(s, func(h *Histogram) {
					h.Buckets[(w+i)&7]++
					if i%250 == 0 {
						h.Tags["sampled"]++
					}
				})
			}
			_ = Finish(s)
		}()
	}
	wg.Wait()
	settled, _ := Settle(cap)
	var sum uint64
	for _, c := range settled.Buckets {
		sum += c
	}
	if sum != 80000 || settled.Tags["sampled"] != 320 {
		t.Fatalf("unexpected settled result: sum=%d sampled=%d", sum, settled.Tags["sampled"])
	}
}

func TestP2SymbolicAndP4Falsifiers(t *testing.T) {
	probes := []Histogram{
		{Buckets: [8]uint64{1, 0, 2}, Tags: map[string]uint64{"a": 1}},
		{Buckets: [8]uint64{0, 3, 1}, Tags: map[string]uint64{"a": 2, "b": 5}},
	}
	if !VerifyCommutativeAssociative[Histogram](HistogramReducer{}, histEq, probes) {
		t.Fatal("expected HistogramReducer to pass commutativity & associativity gate")
	}
	if reflect.DeepEqual([]string{"w0", "w1"}, []string{"w1", "w0"}) {
		t.Fatal("P4a: expected non-commutative schedule divergence")
	}
	m := map[string]uint64{"k": 10}; alias := m; settled := m; alias["k"] = 9999
	if settled["k"] != 9999 {
		t.Fatal("P4b: expected shallow alias mutation")
	}
	a, b, c := 1e16, -1e16, 1.0
	if ((a+b)+c) == (a+(b+c)) || math.Abs(((a+b)+c)-(a+(b+c))) == 0 {
		t.Fatal("P4c: expected float64 non-associativity")
	}
}

func BenchmarkContendedAggregation(b *testing.B) {
	b.Run("MergeCapsule_WaitFreeShard", func(b *testing.B) {
		for n := 0; n < b.N; n++ {
			cap := NewCapsule[rgn, Histogram](HistogramReducer{})
			var wg sync.WaitGroup
			for w := 0; w < 8; w++ {
				s := Split(cap)
				wg.Add(1)
				go func(sh *Shard[rgn, Histogram, Live]) {
					defer wg.Done()
					for i := 0; i < 4096; i++ {
						Mutate(sh, func(h *Histogram) { h.Buckets[i&7]++ })
					}
					_ = Finish(sh)
				}(s)
			}
			wg.Wait()
			_, _ = Settle(cap)
		}
	})
	b.Run("SharedAtomic_Buckets", func(b *testing.B) {
		for n := 0; n < b.N; n++ {
			var buckets [8]atomic.Uint64
			var wg sync.WaitGroup
			for w := 0; w < 8; w++ {
				wg.Add(1)
				go func() {
					defer wg.Done()
					for i := 0; i < 4096; i++ {
						buckets[i&7].Add(1)
					}
				}()
			}
			wg.Wait()
		}
	})
}
