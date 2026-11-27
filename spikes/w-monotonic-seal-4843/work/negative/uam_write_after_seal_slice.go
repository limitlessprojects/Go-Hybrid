//go:build ignore

package main

import "wseal"

type rgn struct{}

func main() {
	s := wseal.NewOwnSlice[rgn, int](2, 4)
	wseal.SetSlice(&s, 0, 10)
	moved, _ := wseal.SealSlice(s)
	// Must fail `go build`: `moved` has state `wseal.Moved`, not `wseal.Live`.
	wseal.SetSlice(&moved, 0, 99)
}
