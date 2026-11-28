package wseal

import (
	"fmt"
	"sync"
	"sync/atomic"
	"testing"
)

var benchSink atomic.Int64

// BenchmarkSealTransition_10kNodes measures the O(1) transition cost of
// `SealMap` and `SealSlice` on a 10,000-entry structure vs W-005 deep-cloning.
func BenchmarkSealTransition_10kNodes(b *testing.B) {
	const nodes = 10000
	baseMap := make(map[string]int, nodes)
	baseSlice := make([]int, nodes, nodes+64)
	for i := 0; i < nodes; i++ {
		k := fmt.Sprintf("node-%05d", i)
		baseMap[k] = i
		baseSlice[i] = i
	}

	b.Run("SealMap_O1_ZeroCopy_10kNodes", func(b *testing.B) {
		b.ReportAllocs()
		var acc int
		for n := 0; n < b.N; n++ {
			own := OwnMap[routeRgn, string, int, Live]{data: baseMap}
			_, sealed := SealMap(own)
			acc += sealed.Len()
		}
		benchSink.Store(int64(acc))
	})

	b.Run("W005_DeepCloneMap_10kNodes", func(b *testing.B) {
		b.ReportAllocs()
		var acc int
		for n := 0; n < b.N; n++ {
			cloned := make(map[string]int, len(baseMap))
			for k, v := range baseMap {
				cloned[k] = v
			}
			acc += len(cloned)
		}
		benchSink.Store(int64(acc))
	})

	b.Run("SealSlice_O1_Clip_10kNodes", func(b *testing.B) {
		b.ReportAllocs()
		var acc int
		for n := 0; n < b.N; n++ {
			own := OwnSlice[sliceRgn, int, Live]{data: baseSlice}
			_, sealed := SealSlice(own)
			acc += sealed.Cap()
		}
		benchSink.Store(int64(acc))
	})

	b.Run("W005_DeepCloneSlice_10kNodes", func(b *testing.B) {
		b.ReportAllocs()
		var acc int
		for n := 0; n < b.N; n++ {
			cloned := make([]int, len(baseSlice))
			copy(cloned, baseSlice)
			acc += len(cloned)
		}
		benchSink.Store(int64(acc))
	})
}

// BenchmarkBroadcast8Tasks_10kNodes compares 8-worker fan-out lookup over a
// 10,000-node routing table across:
//  1. `SealMap` zero-copy Class-4b broadcast (0 clones, 0 locks)
//  2. `W-005` per-task deep-clone message isolation (8 x 10k-node clones)
//  3. `sync.RWMutex` shared map (`RLock` / `RUnlock` cache-line contention)
func BenchmarkBroadcast8Tasks_10kNodes(b *testing.B) {
	const nodes = 10000
	const workers = 8
	const lookupsPerWorker = 512

	baseMap := make(map[int]int, nodes)
	for i := 0; i < nodes; i++ {
		baseMap[i] = i*7 + 3
	}

	b.Run("SealMove_ZeroCopy_8Workers", func(b *testing.B) {
		b.ReportAllocs()
		for n := 0; n < b.N; n++ {
			own := OwnMap[routeRgn, int, int, Live]{data: baseMap}
			_, snap := SealMap(own)
			var wg sync.WaitGroup
			for w := 0; w < workers; w++ {
				wg.Add(1)
				go func(id int, tbl SealedMap[int, int]) {
					defer wg.Done()
					sum := 0
					for i := 0; i < lookupsPerWorker; i++ {
						v, _ := tbl.Get((id*997 + i) % nodes)
						sum += v
					}
					benchSink.Add(int64(sum))
				}(w, snap)
			}
			wg.Wait()
		}
	})

	b.Run("W005_DeepClonePerTask_8Workers", func(b *testing.B) {
		b.ReportAllocs()
		for n := 0; n < b.N; n++ {
			var wg sync.WaitGroup
			for w := 0; w < workers; w++ {
				cloned := make(map[int]int, len(baseMap))
				for k, v := range baseMap {
					cloned[k] = v
				}
				wg.Add(1)
				go func(id int, tbl map[int]int) {
					defer wg.Done()
					sum := 0
					for i := 0; i < lookupsPerWorker; i++ {
						sum += tbl[(id*997+i)%nodes]
					}
					benchSink.Add(int64(sum))
				}(w, cloned)
			}
			wg.Wait()
		}
	})

	b.Run("RWMutex_RLockContended_8Workers", func(b *testing.B) {
		b.ReportAllocs()
		var mu sync.RWMutex
		for n := 0; n < b.N; n++ {
			var wg sync.WaitGroup
			for w := 0; w < workers; w++ {
				wg.Add(1)
				go func(id int) {
					defer wg.Done()
					sum := 0
					for i := 0; i < lookupsPerWorker; i++ {
						mu.RLock()
						v := baseMap[(id*997+i)%nodes]
						mu.RUnlock()
						sum += v
					}
					benchSink.Add(int64(sum))
				}(w)
			}
			wg.Wait()
		}
	})
}
