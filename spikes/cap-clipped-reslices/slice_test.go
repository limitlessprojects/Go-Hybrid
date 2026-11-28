package capbench

import "testing"

var headerSink []int
var sumSink int
var low = 7
var high = 768

func BenchmarkSlice(b *testing.B) {
	s := make([]int, 1024)
	for i := range s {
		s[i] = i + 1
	}
	st := &store{}
	copy(st.data[:], s)
	p := &st.data
	b.Run("TwoIndex", func(b *testing.B) {
		var sum int
		for i := 0; i < b.N; i++ {
			v := View(s, low, high)
			headerSink = v
			sum += v[i&255]
		}
		sumSink = sum
	})
	b.Run("Prefix", func(b *testing.B) {
		var sum int
		for i := 0; i < b.N; i++ {
			v := Prefix(s, high)
			headerSink = v
			sum += v[i&255]
		}
		sumSink = sum
	})
	b.Run("Suffix", func(b *testing.B) {
		var sum int
		for i := 0; i < b.N; i++ {
			v := Suffix(s, low)
			headerSink = v
			sum += v[i&255]
		}
		sumSink = sum
	})
	b.Run("Whole", func(b *testing.B) {
		var sum int
		for i := 0; i < b.N; i++ {
			v := Whole(s)
			headerSink = v
			sum += v[i&255]
		}
		sumSink = sum
	})
	b.Run("Array", func(b *testing.B) {
		var sum int
		for i := 0; i < b.N; i++ {
			v := ArrayView(st, low, high)
			headerSink = v
			sum += v[i&255]
		}
		sumSink = sum
	})
	b.Run("Pointer", func(b *testing.B) {
		var sum int
		for i := 0; i < b.N; i++ {
			v := PointerView(p, low, high)
			headerSink = v
			sum += v[i&255]
		}
		sumSink = sum
	})
	b.Run("Nested", func(b *testing.B) {
		var sum int
		for i := 0; i < b.N; i++ {
			v := Nested(s, low, high)
			headerSink = v
			sum += v[i&255]
		}
		sumSink = sum
	})
}
