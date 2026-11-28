//go:build ignore

package main

import "wseal"

type rgn struct{}

func main() {
	m := wseal.NewOwnMap[rgn, string, int](4)
	wseal.PutMap(&m, "a", 1)
	moved, _ := wseal.SealMap(m)
	// Must fail `go build`: `moved` has state `wseal.Moved`, not `wseal.Live`.
	wseal.PutMap(&moved, "b", 2)
}
