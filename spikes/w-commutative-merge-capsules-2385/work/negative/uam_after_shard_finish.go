//go:build ignore
package main
import "w2385/merge"
type rgn struct{}
func main() {
	c := merge.NewCapsule[rgn, merge.Histogram](merge.HistogramReducer{})
	s := merge.Split(c)
	moved := merge.Finish(s)
	merge.Mutate(moved, func(h *merge.Histogram) { h.Buckets[0]++ })
}
