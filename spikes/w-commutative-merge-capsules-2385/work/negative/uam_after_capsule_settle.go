//go:build ignore
package main
import "w2385/merge"
type rgn struct{}
func main() {
	c := merge.NewCapsule[rgn, merge.Histogram](merge.HistogramReducer{})
	_, settled := merge.Settle(c)
	_ = merge.Split(settled)
}
