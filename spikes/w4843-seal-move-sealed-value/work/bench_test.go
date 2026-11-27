package w4843sealmove

import "testing"

const benchNodes = 10000

func buildBenchMap() map[int]int {
	m := make(map[int]int, benchNodes)
	for i := 0; i < benchNodes; i++ {
		m[i] = i * 7
	}
	return m
}

func buildBenchSlice() []int {
	s := make([]int, benchNodes)
	for i := range s {
		s[i] = i * 7
	}
	return s
}

func cloneMap(m map[int]int) map[int]int {
	out := make(map[int]int, len(m))
	for k, v := range m {
		out[k] = v
	}
	return out
}

func cloneSlice(s []int) []int {
	out := make([]int, len(s))
	copy(out, s)
	return out
}

// BenchmarkSealMap10kNodes measures the P2 claim: sealing a 10,000-entry map
// is an O(1) header wrap, not an O(N) traversal.
func BenchmarkSealMap10kNodes(b *testing.B) {
	m := buildBenchMap()
	b.ReportAllocs()
	for i := 0; i < b.N; i++ {
		_ = SealMap(m)
	}
}

// BenchmarkCloneMap10kNodes is the W-005 message-isolation baseline this
// proposal claims to beat for the broadcast-without-mutation use case.
func BenchmarkCloneMap10kNodes(b *testing.B) {
	m := buildBenchMap()
	b.ReportAllocs()
	for i := 0; i < b.N; i++ {
		_ = cloneMap(m)
	}
}

// BenchmarkSealSlice10kNodes measures the O(1) 3-index capacity-clip
// transition (R1) for a 10,000-element slice.
func BenchmarkSealSlice10kNodes(b *testing.B) {
	s := buildBenchSlice()
	b.ReportAllocs()
	for i := 0; i < b.N; i++ {
		_ = SealSlice(s)
	}
}

// BenchmarkCloneSlice10kNodes is the deep-copy baseline for the same slice.
func BenchmarkCloneSlice10kNodes(b *testing.B) {
	s := buildBenchSlice()
	b.ReportAllocs()
	for i := 0; i < b.N; i++ {
		_ = cloneSlice(s)
	}
}

// BenchmarkBroadcast8WorkersSealMove measures a seal-once / read-many
// fan-out: one SealMap plus 8 goroutine spawns, each doing 512 lookups.
func BenchmarkBroadcast8WorkersSealMove(b *testing.B) {
	m := buildBenchMap()
	b.ReportAllocs()
	for i := 0; i < b.N; i++ {
		sealed := SealMap(m)
		done := make(chan struct{}, 8)
		for w := 0; w < 8; w++ {
			go func() {
				for k := 0; k < 512; k++ {
					_, _ = sealed.Get(k)
				}
				done <- struct{}{}
			}()
		}
		for w := 0; w < 8; w++ {
			<-done
		}
	}
}

// BenchmarkBroadcast8WorkersCloneBaseline is the W-005-shaped baseline:
// clone the map once per task instead of sealing it once and sharing it.
func BenchmarkBroadcast8WorkersCloneBaseline(b *testing.B) {
	m := buildBenchMap()
	b.ReportAllocs()
	for i := 0; i < b.N; i++ {
		done := make(chan struct{}, 8)
		for w := 0; w < 8; w++ {
			go func() {
				mine := cloneMap(m)
				for k := 0; k < 512; k++ {
					_ = mine[k]
				}
				done <- struct{}{}
			}()
		}
		for w := 0; w < 8; w++ {
			<-done
		}
	}
}
